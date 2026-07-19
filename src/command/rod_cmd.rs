//! The rod composite command: one gesture, one undo step, several records.
//!
//! A rigid rod is a capsule body plus up to two end joints; a flexure rod
//! is an elastica joint plus up to two tip bodies. Either way the strut
//! tool must author them **atomically** — undo removes the whole rod, redo
//! restores it with the same ids (precedent:
//! [`DuplicateCommand`](crate::command::spawn::DuplicateCommand)).

use crate::command::snapshot::{BodyRecord, JointRecord};
use crate::command::{CommandError, GameCommand, resolve};
use bevy::prelude::*;

/// Everything one rod gesture authors: the bodies (a rigid rod's capsule,
/// or a flexure rod's tip circles) and the joints (end constraints, or
/// the elastica element).
#[derive(Debug, Clone, Reflect)]
pub struct RodSpec {
    /// Bodies to author (ids minted by the tool, stable across redo).
    pub bodies: Vec<BodyRecord>,
    /// Joints to author, referencing `bodies` and/or pre-existing bodies.
    pub joints: Vec<JointRecord>,
}

/// Spawns a rod's records atomically; undo despawns them all by id.
#[derive(Debug)]
pub struct SpawnRodCommand {
    /// The records to author.
    pub spec: RodSpec,
}

impl GameCommand for SpawnRodCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.spec.bodies.is_empty() && self.spec.joints.is_empty() {
            return Err(CommandError::NoEffect);
        }
        // Validate everything before touching the world so a failure
        // leaves it unchanged.
        for body in &self.spec.bodies {
            body.shape.validate()?;
        }
        for joint in &self.spec.joints {
            for id in joint.def.referenced_bodies() {
                let spawning = self.spec.bodies.iter().any(|b| b.id == id);
                if !spawning {
                    resolve(world, id)?;
                }
            }
        }
        for body in &self.spec.bodies {
            body.spawn(world);
        }
        for joint in &self.spec.joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        for joint in &self.spec.joints {
            let entity = resolve(world, joint.id)?;
            world.despawn(entity);
        }
        for body in &self.spec.bodies {
            let entity = resolve(world, body.id)?;
            world.despawn(entity);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::SPAWN_ROD
    }
}

/// Converts a rod between its two representations, atomically and
/// undoably:
///
/// - **enable** (`target` = the rigid capsule body's id): the capsule and
///   its end joints become one elastica joint; ends that had no joint get
///   a small circle tip body (marked [`RodTip`](crate::domain::rod::RodTip)).
/// - **disable** (`target` = the elastica joint's id): the elastica and
///   its tip bodies become a rigid capsule with weld/hinge end joints.
///
/// Records are staged on first apply and replayed on redo, so every id is
/// stable across undo/redo.
#[derive(Debug)]
pub struct SetRodFlexureCommand {
    /// The rigid rod body (enable) or elastica joint (disable) to convert.
    pub target: crate::core::ids::StableId,
    /// `true` = rigid → flexure; `false` = flexure → rigid.
    pub enable: bool,
    staged: Option<Staged>,
}

/// The records the conversion removes and creates (captured once).
#[derive(Debug)]
struct Staged {
    removed_bodies: Vec<BodyRecord>,
    removed_joints: Vec<JointRecord>,
    added: RodSpec,
}

impl SetRodFlexureCommand {
    /// Builds the conversion for `target`.
    pub fn new(target: crate::core::ids::StableId, enable: bool) -> Self {
        Self {
            target,
            enable,
            staged: None,
        }
    }

    fn stage(&self, world: &mut World) -> Result<Staged, CommandError> {
        if self.enable {
            stage_enable(world, self.target)
        } else {
            stage_disable(world, self.target)
        }
    }
}

impl GameCommand for SetRodFlexureCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.staged.is_none() {
            self.staged = Some(self.stage(world)?);
        }
        let Some(staged) = &self.staged else {
            return Err(CommandError::NoEffect);
        };
        for joint in &staged.removed_joints {
            let entity = resolve(world, joint.id)?;
            world.despawn(entity);
        }
        for body in &staged.removed_bodies {
            let entity = resolve(world, body.id)?;
            world.despawn(entity);
        }
        for body in &staged.added.bodies {
            body.spawn(world);
        }
        for joint in &staged.added.joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let Some(staged) = &self.staged else {
            return Err(CommandError::NoEffect);
        };
        for joint in &staged.added.joints {
            let entity = resolve(world, joint.id)?;
            world.despawn(entity);
        }
        for body in &staged.added.bodies {
            let entity = resolve(world, body.id)?;
            world.despawn(entity);
        }
        for body in &staged.removed_bodies {
            body.spawn(world);
        }
        for joint in &staged.removed_joints {
            joint.spawn(world);
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::SET_ROD_FLEXURE
    }
}

/// A flexure tip body: a small circle at the beam's free end, inheriting
/// the rod's physics/appearance/depth.
fn tip_record(rod: &BodyRecord, pos: Vec2, radius: f32) -> BodyRecord {
    BodyRecord {
        id: crate::core::ids::StableId::new(),
        pose: crate::core::units::PosRot { pos, rot: 0.0 },
        shape: crate::domain::shape::ShapeDef::Circle { radius },
        physics: rod.physics,
        appearance: rod.appearance,
        depth: rod.depth,
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        rod: None,
        rod_tip: true,
    }
}

/// Stages rigid → flexure: the capsule + end joints out, one elastica
/// joint (+ tip bodies for unattached ends) in.
fn stage_enable(
    world: &mut World,
    rod_id: crate::core::ids::StableId,
) -> Result<Staged, CommandError> {
    use crate::domain::joint::{JointCommon, JointDef, JointKind};
    use crate::domain::rod::RodEndKind;
    use crate::domain::shape::ShapeDef;

    let entity = resolve(world, rod_id)?;
    let record = BodyRecord::capture(world, entity).ok_or(CommandError::NoEffect)?;
    let ShapeDef::Capsule {
        half_length,
        radius,
    } = record.shape
    else {
        return Err(CommandError::NoEffect);
    };
    let rod = record.rod.unwrap_or(crate::domain::rod::Rod {
        end_a: RodEndKind::Hinge,
        end_b: RodEndKind::Hinge,
    });
    let removed_joints = crate::command::joint_cmd::joints_referencing(world, &[rod_id])
        .into_iter()
        .map(|(_, joint)| joint)
        .collect::<Vec<_>>();

    let rot = Vec2::from_angle(record.pose.rot);
    let chord_angle = record.pose.rot;
    let ends_world = [
        record.pose.pos + rot.rotate(Vec2::new(-half_length, 0.0)),
        record.pose.pos + rot.rotate(Vec2::new(half_length, 0.0)),
    ];

    // Each end: reuse the removed end joint's attachment, else mint a tip.
    let mut bodies = Vec::new();
    let mut resolve_end = |world: &mut World,
                           end_world: Vec2,
                           is_a: bool|
     -> (crate::core::ids::StableId, Vec2, f32) {
        let attached = removed_joints.iter().find(|j| {
            j.def.body_a == rod_id && (j.def.anchor_a.x < 0.0) == is_a && j.def.body_b.is_some()
        });
        if let Some(joint) = attached
            && let Some(target) = joint.def.body_b
            && let Some(target_entity) =
                world.resource::<crate::core::ids::IdIndex>().entity(target)
            && let Some(transform) = world.get::<bevy::prelude::Transform>(target_entity)
        {
            let pose = crate::core::units::PosRot::from_transform(transform);
            return (target, joint.def.anchor_b, pose.rot);
        }
        let tip = tip_record(&record, end_world, radius);
        let id = tip.id;
        bodies.push(tip);
        (id, Vec2::ZERO, 0.0)
    };
    let (id_a, anchor_a, rot_a) = resolve_end(world, ends_world[0], true);
    let (id_b, anchor_b, rot_b) = resolve_end(world, ends_world[1], false);
    if id_a == id_b {
        return Err(CommandError::NoEffect);
    }

    let joint = JointRecord {
        id: crate::core::ids::StableId::new(),
        def: JointDef {
            kind: JointKind::Elastica {
                length: 2.0 * half_length,
                young_modulus: crate::domain::joint::DEFAULT_FLEXURE_E,
                thickness: 2.0 * radius,
                damping_ratio: crate::domain::joint::DEFAULT_FLEXURE_DAMPING,
                end_a: rod.end_a,
                end_b: rod.end_b,
                tangent_a: chord_angle - rot_a,
                tangent_b: chord_angle - rot_b,
            },
            common: JointCommon {
                collide_connected: false,
            },
            body_a: id_a,
            body_b: Some(id_b),
            anchor_a,
            anchor_b,
            rest_rot_a: rot_a,
            rest_rot_b: rot_b,
        },
    };
    Ok(Staged {
        removed_bodies: vec![record],
        removed_joints,
        added: RodSpec {
            bodies,
            joints: vec![joint],
        },
    })
}

/// One rigid end joint for the flexure→rigid conversion.
fn rigid_end_joint(
    rod_id: crate::core::ids::StableId,
    chord_angle: f32,
    target: crate::core::ids::StableId,
    rod_anchor: Vec2,
    target_anchor: Vec2,
    target_rot: f32,
    kind: crate::domain::rod::RodEndKind,
) -> JointRecord {
    use crate::domain::joint::{JointCommon, JointDef, JointKind};
    use crate::domain::rod::RodEndKind;
    JointRecord {
        id: crate::core::ids::StableId::new(),
        def: JointDef {
            kind: match kind {
                RodEndKind::Fixed => JointKind::Weld,
                RodEndKind::Hinge => JointKind::Hinge {
                    limits: None,
                    motor: None,
                },
            },
            common: JointCommon {
                collide_connected: false,
            },
            body_a: rod_id,
            body_b: Some(target),
            anchor_a: rod_anchor,
            anchor_b: target_anchor,
            rest_rot_a: chord_angle,
            rest_rot_b: target_rot,
        },
    }
}

/// A rigid rod capsule body record (the flexure→rigid conversion's
/// replacement body; the strut tool's default look and density).
fn rigid_rod_record(
    id: crate::core::ids::StableId,
    pos: Vec2,
    rot: f32,
    half_length: f32,
    thickness: f32,
    (end_a, end_b): (
        crate::domain::rod::RodEndKind,
        crate::domain::rod::RodEndKind,
    ),
) -> BodyRecord {
    BodyRecord {
        id,
        pose: crate::core::units::PosRot { pos, rot },
        shape: crate::domain::shape::ShapeDef::Capsule {
            half_length,
            radius: (thickness / 2.0).max(0.5),
        },
        physics: crate::domain::props::BodyPhysics {
            density: avian2d::prelude::ColliderDensity(5.0),
            ..Default::default()
        },
        appearance: crate::domain::appearance::Appearance {
            fill: crate::domain::appearance::Rgba {
                r: 0.02,
                g: 0.02,
                b: 0.02,
                a: 1.0,
            },
            ..Default::default()
        },
        depth: crate::domain::depth::DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
        rod: Some(crate::domain::rod::Rod { end_a, end_b }),
        rod_tip: false,
    }
}

/// Stages flexure → rigid: the elastica + its tip bodies out, a capsule
/// with weld/hinge end joints in.
fn stage_disable(
    world: &mut World,
    joint_id: crate::core::ids::StableId,
) -> Result<Staged, CommandError> {
    use crate::domain::joint::JointKind;

    let entity = resolve(world, joint_id)?;
    let joint = JointRecord::capture(world, entity).ok_or(CommandError::NoEffect)?;
    let JointKind::Elastica {
        thickness,
        end_a,
        end_b,
        ..
    } = joint.def.kind
    else {
        return Err(CommandError::NoEffect);
    };
    let id_b = joint.def.body_b.ok_or(CommandError::NoEffect)?;

    // Current world anchors define the rigid rod's chord.
    let anchor_world = |world: &mut World,
                        id: crate::core::ids::StableId,
                        local: Vec2|
     -> Result<(Vec2, f32, bool), CommandError> {
        let entity = resolve(world, id)?;
        let transform = world
            .get::<bevy::prelude::Transform>(entity)
            .ok_or(CommandError::MissingEntity(id))?;
        let pose = crate::core::units::PosRot::from_transform(transform);
        let is_tip = world.get::<crate::domain::rod::RodTip>(entity).is_some();
        Ok((
            pose.pos + Vec2::from_angle(pose.rot).rotate(local),
            pose.rot,
            is_tip,
        ))
    };
    let (pa, rot_a, tip_a) = anchor_world(world, joint.def.body_a, joint.def.anchor_a)?;
    let (pb, rot_b, tip_b) = anchor_world(world, id_b, joint.def.anchor_b)?;
    let length = pa.distance(pb).max(1.0);
    let chord_angle = (pb - pa).to_angle();

    // Tip bodies are absorbed into the rigid rod; real attachments keep
    // their end joints.
    let mut removed_bodies = Vec::new();
    let mut capture_tip = |world: &mut World,
                           id: crate::core::ids::StableId,
                           is_tip: bool|
     -> Result<(), CommandError> {
        if is_tip {
            let entity = resolve(world, id)?;
            if let Some(record) = BodyRecord::capture(world, entity) {
                removed_bodies.push(record);
            }
        }
        Ok(())
    };
    capture_tip(world, joint.def.body_a, tip_a)?;
    capture_tip(world, id_b, tip_b)?;

    let rod_id = crate::core::ids::StableId::new();
    let half = length / 2.0;
    let body = rigid_rod_record(
        rod_id,
        (pa + pb) / 2.0,
        chord_angle,
        half,
        thickness,
        (end_a, end_b),
    );

    // End joints only for real (non-tip) attachments.
    let ends = [
        (
            joint.def.body_a,
            tip_a,
            Vec2::new(-half, 0.0),
            joint.def.anchor_a,
            rot_a,
            end_a,
        ),
        (
            id_b,
            tip_b,
            Vec2::new(half, 0.0),
            joint.def.anchor_b,
            rot_b,
            end_b,
        ),
    ];
    let joints = ends
        .into_iter()
        .filter(|(_, is_tip, ..)| !is_tip)
        .map(|(target, _, rod_anchor, target_anchor, target_rot, kind)| {
            rigid_end_joint(
                rod_id,
                chord_angle,
                target,
                rod_anchor,
                target_anchor,
                target_rot,
                kind,
            )
        })
        .collect();

    Ok(Staged {
        removed_bodies,
        removed_joints: vec![joint],
        added: RodSpec {
            bodies: vec![body],
            joints,
        },
    })
}
