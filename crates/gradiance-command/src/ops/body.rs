//! Operations on **existing bodies**: their pose, their size, their removal
//! and the inspector's physics fields. Each names its body with a [`Target`]
//! resolved against the context's snapshot, which also holds the old value an
//! undoable intent needs.

use super::{bodies_of, body_of, body_set};
use crate::intent::TransformChange;
use crate::intent::name;
use crate::intent::{CommitTransformIntent, DeleteIntent, PropertyEditIntent, ScaleIntent};
use crate::property::{PropertyChange, PropertyValue, fluid_changes};
use crate::snapshot::SceneSnapshot;
use avian2d::prelude::{ColliderDensity, Friction, Restitution, RigidBody};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::fluid::Fluid;
use gradiance_domain::motion::InitialMotion;
use gradiance_domain::props::BodyPhysics;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_units::{Angle, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<Place>();
    registry.register::<Delete>();
    registry.register::<Scale>();
    registry.register::<MirrorH>();
    registry.register::<MirrorV>();
    registry.register::<SetFriction>();
    registry.register::<SetRestitution>();
    registry.register::<SetDensity>();
    registry.register::<SetStatic>();
    registry.register::<SetColor>();
    registry.register::<SetFluid>();
    registry.register::<SetVelocity>();
    registry.register::<SetSpin>();
    registry.register::<SetArrows>();
    registry.register::<SetIndex>();
}

/// Move the body `i` to (`x`, `y`) and rotate it to `angle`: one undo step.
/// A target that names no body, or a move to where the body already is, does
/// nothing (no empty undo step).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Place {
    /// The body: an index in id order, an id, or a point on it.
    pub i: Target,
    /// New centre x.
    pub x: Length,
    /// New centre y.
    pub y: Length,
    /// New rotation.
    pub angle: Angle,
}

impl Operation for Place {
    const META: OpMeta = OpMeta {
        id: "body.place",
        lisp: "place",
        doc: "Move the i-th body to (x, y) and rotate it to `angle` radians — one undo step.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::COMMIT_TRANSFORM),
        example: r#"{"i": 0, "x": 1.5, "y": 0.25, "angle": 0.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let Some(body) = body_of(ctx, scene, &self.i)? else {
            return Ok(OpOutput::none());
        };
        let old = body.pose();
        let new = PosRot {
            pos: Vec2::new(self.x.0, self.y.0),
            rot: self.angle.0,
        };
        if new == old {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(CommitTransformIntent {
            changes: vec![TransformChange {
                id: body.id,
                old,
                new,
            }],
        }))
    }
}

/// Delete the body `i` (`"selection"`: every selected body and node). An id
/// may also name a behaviour node. A target that names nothing does nothing.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    /// The body.
    pub i: Target,
}

impl Operation for Delete {
    const META: OpMeta = OpMeta {
        id: "body.delete",
        lisp: "delete",
        doc: "Delete the i-th body (id order, same index as body-x) — undoable.",
        seam: Seam::Edit,
        menu: Some("Edit/Delete selection"),
        chord: Some("Delete"),
        undo: Some(name::DELETE),
        example: r#"{"i": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let known = |id: &StableId| scene.get(*id).is_some() || scene.nodes.contains(id);
        let targets = match self.i {
            Target::Selection => ctx
                .selection()?
                .objects
                .iter()
                .copied()
                .filter(known)
                .collect(),
            Target::Id(id) if scene.nodes.contains(&id) => vec![id],
            other => bodies_of(ctx, scene, &other)?,
        };
        if targets.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(DeleteIntent { targets }))
    }
}

/// Resize the body `i` by (`fx`, `fy`) about its own centre, along its own
/// axes. Local axes rather than global: "twice as wide" means along the
/// body's width, and a global-axis scale of a rotated box would shear it into
/// a polygon. A factor that is not positive does nothing: zero collapses the
/// body for good and a negative one mirrors it, and neither is a resize.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct Scale {
    /// The body.
    pub i: Target,
    /// Factor along the body's own x axis.
    pub fx: f32,
    /// Factor along the body's own y axis.
    pub fy: f32,
}

impl Operation for Scale {
    const META: OpMeta = OpMeta {
        id: "body.scale",
        lisp: "scale",
        doc: "Resize the i-th body by (fx, fy) about its own centre, along its own axes.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::SCALE),
        example: r#"{"i": 0, "fx": 2, "fy": 1.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let positive = self.fx > 0.0 && self.fy > 0.0;
        let Some(body) = body_of(ctx, scene, &self.i)?.filter(|_| positive) else {
            return Ok(OpOutput::none());
        };
        Ok(OpOutput::none().message(ScaleIntent {
            targets: vec![body.id],
            pivot: body.pos,
            frame_rot: body.rot,
            factors: Vec2::new(self.fx, self.fy),
        }))
    }
}

/// One mirror of the bodies `targets` names, across the vertical (`x`
/// flips) or horizontal (`y` flips) line through their centre: the existing
/// scale with one factor of −1 in world axes (`ScaleCommand`, which turns
/// each body to its reflected rotation and takes its joints and nodes
/// along). The centre is the middle of the bodies' world bounds (half-planes
/// are unbounded and left out), else of their positions. Nothing named is a
/// quiet no-op, as Delete with nothing selected.
fn mirror(ctx: &OpContext, targets: &[Target], factors: Vec2) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let targets = body_set(ctx, scene, targets)?;
    let bodies: Vec<&crate::snapshot::BodySnapshot> =
        targets.iter().filter_map(|id| scene.get(*id)).collect();
    let Some(pivot) = mirror_pivot(&bodies) else {
        return Ok(OpOutput::none());
    };
    Ok(OpOutput::none().message(ScaleIntent {
        targets,
        pivot,
        frame_rot: 0.0,
        factors,
    }))
}

/// The middle of the bodies' world bounds; half-planes are left out, and
/// with nothing bounded the middle of their positions. `None` for no bodies.
fn mirror_pivot(bodies: &[&crate::snapshot::BodySnapshot]) -> Option<Vec2> {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for body in bodies.iter().filter(|b| !b.shape.contains_half_plane()) {
        let rot = Vec2::from_angle(body.rot);
        for v in &gradiance_geometry::polygonize::polygonize(&body.shape).outline {
            let w = body.pos + rot.rotate(*v);
            min = min.min(w);
            max = max.max(w);
        }
    }
    if min.x <= max.x {
        return Some((min + max) / 2.0);
    }
    for body in bodies {
        min = min.min(body.pos);
        max = max.max(body.pos);
    }
    (min.x <= max.x).then(|| (min + max) / 2.0)
}

/// Mirror the listed bodies left to right, across the vertical line through
/// their centre — one undo step. Their joints, anchors and nodes mirror with
/// them (a hinge's limits and motor turn over when all its bodies do).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct MirrorH {
    /// The bodies: a list of targets (`(list i …)` in Lisp).
    pub targets: Vec<Target>,
}

impl Operation for MirrorH {
    const META: OpMeta = OpMeta {
        id: "body.mirror_h",
        lisp: "mirror-h",
        doc: "Mirror the listed bodies left to right across the vertical line through their centre, with their joints and nodes — one undo step.",
        seam: Seam::Edit,
        menu: Some("Edit/Mirror horizontally"),
        chord: None,
        undo: Some(name::MIRROR),
        example: r#"{"targets": [0, 1]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        mirror(ctx, &self.targets, Vec2::new(-1.0, 1.0))
    }
}

/// Mirror the listed bodies top to bottom, across the horizontal line
/// through their centre — one undo step, as [`MirrorH`].
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct MirrorV {
    /// The bodies: a list of targets (`(list i …)` in Lisp).
    pub targets: Vec<Target>,
}

impl Operation for MirrorV {
    const META: OpMeta = OpMeta {
        id: "body.mirror_v",
        lisp: "mirror-v",
        doc: "Mirror the listed bodies top to bottom across the horizontal line through their centre, with their joints and nodes — one undo step.",
        seam: Seam::Edit,
        menu: Some("Edit/Mirror vertically"),
        chord: None,
        undo: Some(name::MIRROR),
        example: r#"{"targets": [0, 1]}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        mirror(ctx, &self.targets, Vec2::new(1.0, -1.0))
    }
}

/// One property edit over the bodies `target` names: `build` turns a body's
/// physics into the old and new value. A body already at the new value is
/// left out, so a no-op is no undo step (the guard `place` has).
fn property_edit(
    ctx: &OpContext,
    target: &Target,
    build: impl Fn(&BodyPhysics) -> (PropertyValue, PropertyValue),
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let changes: Vec<PropertyChange> = bodies_of(ctx, scene, target)?
        .into_iter()
        .filter_map(|id| {
            let (old, new) = build(&scene.get(id)?.physics);
            (old != new).then_some(PropertyChange { id, old, new })
        })
        .collect();
    if changes.is_empty() {
        return Ok(OpOutput::none());
    }
    Ok(OpOutput::none().message(PropertyEditIntent { changes }))
}

/// Set the body's Coulomb friction. Both coefficients move together, as the
/// inspector does: authoring two numbers that are almost always equal is
/// friction the UI deliberately does not expose.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetFriction {
    /// The body.
    pub i: Target,
    /// The coefficient.
    pub v: f32,
}

impl Operation for SetFriction {
    const META: OpMeta = OpMeta {
        id: "body.set_friction",
        lisp: "set-friction",
        doc: "Set the i-th body's Coulomb friction (static and dynamic) — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 0.9}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Friction(physics.friction);
            let new = PropertyValue::Friction(Friction {
                dynamic_coefficient: self.v,
                static_coefficient: self.v,
                ..physics.friction
            });
            (old, new)
        })
    }
}

/// Set the body's bounciness: 0 is dead, 1 perfectly elastic.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetRestitution {
    /// The body.
    pub i: Target,
    /// The coefficient.
    pub v: f32,
}

impl Operation for SetRestitution {
    const META: OpMeta = OpMeta {
        id: "body.set_restitution",
        lisp: "set-restitution",
        doc: "Set the i-th body's bounciness, 0 = dead, 1 = perfectly elastic — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 0.8}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Restitution(physics.restitution);
            let new = PropertyValue::Restitution(Restitution {
                coefficient: self.v,
                ..physics.restitution
            });
            (old, new)
        })
    }
}

/// Set the body's mass density (area x density = mass).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetDensity {
    /// The body.
    pub i: Target,
    /// The density.
    pub v: f32,
}

impl Operation for SetDensity {
    const META: OpMeta = OpMeta {
        id: "body.set_density",
        lisp: "set-density",
        doc: "Set the i-th body's mass density (area x density = mass) — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "v": 3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        property_edit(ctx, &self.i, |physics| {
            let old = PropertyValue::Density(physics.density);
            let new = PropertyValue::Density(ColliderDensity(self.v));
            (old, new)
        })
    }
}

/// Make the body static when `on` is non-zero, dynamic otherwise: a toggle
/// with an explicit argument rather than two verbs that can disagree.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetStatic {
    /// The body.
    pub i: Target,
    /// Non-zero for static.
    pub on: f64,
}

impl Operation for SetStatic {
    const META: OpMeta = OpMeta {
        id: "body.set_static",
        lisp: "set-static",
        doc: "Make the i-th body static when `on` is non-zero, dynamic otherwise — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "on": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let kind = if self.on == 0.0 {
            RigidBody::Dynamic
        } else {
            RigidBody::Static
        };
        property_edit(ctx, &self.i, |physics| {
            (
                PropertyValue::RigidBody(physics.rigid_body),
                PropertyValue::RigidBody(kind),
            )
        })
    }
}

/// Set the body's fill colour from RGB in `[0, 1]` (alpha, border and glow
/// are kept): what a scene script uses for a fixed look, where the
/// id-derived colour would change from run to run.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetColor {
    /// The body.
    pub i: Target,
    /// Red in `[0, 1]`.
    pub r: f32,
    /// Green in `[0, 1]`.
    pub g: f32,
    /// Blue in `[0, 1]`.
    pub b: f32,
}

impl Operation for SetColor {
    const META: OpMeta = OpMeta {
        id: "body.set_color",
        lisp: "set-color",
        doc: "Set the i-th body's fill to (r, g, b), each in 0..1 — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "r": 0.9, "g": 0.3, "b": 0.1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let changes: Vec<PropertyChange> = bodies_of(ctx, scene, &self.i)?
            .into_iter()
            .filter_map(|id| {
                let old = scene.get(id)?.appearance;
                let fill = Rgba {
                    a: old.fill.a,
                    ..Rgba::rgb(
                        self.r.clamp(0.0, 1.0),
                        self.g.clamp(0.0, 1.0),
                        self.b.clamp(0.0, 1.0),
                    )
                };
                let new = Appearance { fill, ..old };
                (old != new).then_some(PropertyChange {
                    id,
                    old: PropertyValue::Appearance(old),
                    new: PropertyValue::Appearance(new),
                })
            })
            .collect();
        if changes.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(PropertyEditIntent { changes }))
    }
}

/// Make the body a fluid region (brief N-7): every dynamic body inside it is
/// pushed up by the weight of the fluid it displaces and damped. A density
/// of 0 or less turns the fluid off.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetFluid {
    /// The body.
    pub i: Target,
    /// The fluid's areal density (kg/m²); 0 or less: not a fluid.
    pub density: f32,
    /// The drag rate (1/s); absent: the body's current drag, or the default.
    pub drag: Option<f32>,
}

impl Operation for SetFluid {
    const META: OpMeta = OpMeta {
        id: "body.set_fluid",
        lisp: "set-fluid",
        doc: "Make the i-th body a fluid of density (kg/m2) and drag (1/s): bodies inside float or sink by their density and are damped; making a fluid also makes it a static sensor. Density 0 turns the fluid off — undoable.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "density": 2, "drag": 2}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let changes: Vec<PropertyChange> = bodies_of(ctx, scene, &self.i)?
            .into_iter()
            .filter_map(|id| scene.get(id))
            .flat_map(|body| {
                let new = (self.density > 0.0).then(|| Fluid {
                    density: gradiance_units::Density(self.density),
                    drag: self
                        .drag
                        .unwrap_or_else(|| body.fluid.unwrap_or_default().drag),
                });
                fluid_changes(
                    body.id,
                    body.fluid,
                    body.physics.sensor,
                    body.physics.rigid_body,
                    new,
                )
            })
            .collect();
        if changes.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(PropertyEditIntent { changes }))
    }
}

/// The property edits that give every body `target` names a new initial
/// motion, `edit` applied to its current one (at rest when it has none).
fn motion_edits(
    ctx: &OpContext,
    target: &Target,
    edit: impl Fn(InitialMotion) -> InitialMotion,
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let changes: Vec<PropertyChange> = bodies_of(ctx, scene, target)?
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter_map(|body| {
            let next = edit(body.motion.unwrap_or_default()).sanitized();
            let new = (!next.is_rest()).then_some(next);
            (new != body.motion).then_some(PropertyChange {
                id: body.id,
                old: PropertyValue::InitialMotion(body.motion),
                new: PropertyValue::InitialMotion(new),
            })
        })
        .collect();
    if changes.is_empty() {
        return Ok(OpOutput::none());
    }
    Ok(OpOutput::none().message(PropertyEditIntent { changes }))
}

/// Set the velocity a body starts a run with (brief S-1).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetVelocity {
    /// The body.
    pub i: Target,
    /// Velocity along x (m/s).
    pub vx: f32,
    /// Velocity along y (m/s).
    pub vy: f32,
}

impl Operation for SetVelocity {
    const META: OpMeta = OpMeta {
        id: "body.set_velocity",
        lisp: "set-velocity",
        doc: "Set the velocity (vx, vy in m/s) the i-th body starts each run with; (0, 0) and no spin: it starts at rest. Applied when play starts from an edited layout, not on resume — undoable, saved.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "vx": 3, "vy": 0}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let v = Vec2::new(self.vx, self.vy);
        motion_edits(ctx, &self.i, |m| InitialMotion {
            velocity: gradiance_units::Velocity2(v),
            ..m
        })
    }
}

/// Set the spin a body starts a run with (brief S-1).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetSpin {
    /// The body.
    pub i: Target,
    /// Angular velocity (rad/s, counter-clockwise).
    pub w: f32,
}

impl Operation for SetSpin {
    const META: OpMeta = OpMeta {
        id: "body.set_spin",
        lisp: "set-spin",
        doc: "Set the spin (rad/s, counter-clockwise) the i-th body starts each run with. Applied when play starts from an edited layout, not on resume — undoable, saved.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "w": 2}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        motion_edits(ctx, &self.i, |m| InitialMotion {
            spin: gradiance_units::AngularVelocity(self.w),
            ..m
        })
    }
}

/// Choose a body's motion arrows (brief S-2b).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetArrows {
    /// The body.
    pub i: Target,
    /// Non-zero: draw its velocity arrow.
    pub velocity: f32,
    /// Non-zero: draw its momentum arrow.
    pub momentum: f32,
    /// Non-zero: draw its net-force arrow.
    pub force: f32,
}

impl Operation for SetArrows {
    const META: OpMeta = OpMeta {
        id: "body.set_arrows",
        lisp: "set-arrows",
        doc: "Choose which motion arrows the i-th body shows while it plays: velocity, momentum, force (non-zero = on) — undoable, saved. The arrow length is Settings > Camera > arrow scale.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "velocity": 1, "momentum": 0, "force": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let arrows = gradiance_domain::appearance::BodyArrows {
            velocity: self.velocity != 0.0,
            momentum: self.momentum != 0.0,
            force: self.force != 0.0,
        };
        let changes: Vec<PropertyChange> = bodies_of(ctx, scene, &self.i)?
            .into_iter()
            .filter_map(|id| scene.get(id))
            .filter(|body| body.appearance.arrows != arrows)
            .map(|body| PropertyChange {
                id: body.id,
                old: PropertyValue::Appearance(body.appearance),
                new: PropertyValue::Appearance(Appearance {
                    arrows,
                    ..body.appearance
                }),
            })
            .collect();
        if changes.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(PropertyEditIntent { changes }))
    }
}

/// Make a body transparent to laser beams with a refractive index (brief
/// S-3), or a mirror again.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetIndex {
    /// The body.
    pub i: Target,
    /// The refractive index (1.33 water, 1.5 glass, held to 1..4); 0 or
    /// less: a mirror again.
    pub n: f32,
}

impl Operation for SetIndex {
    const META: OpMeta = OpMeta {
        id: "body.set_index",
        lisp: "set-index",
        doc: "Give the i-th body a refractive index n (1.33 water, 1.5 glass; held to 1..4): laser beams pass through it, bent by Snell's law, and reflect inside past the critical angle. n 0 or less makes it a mirror again — undoable, saved.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "n": 1.5}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let scene = ctx.get::<SceneSnapshot>()?;
        let new =
            (self.n > 0.0).then(|| gradiance_domain::optics::Optics { index: self.n }.sanitized());
        let changes: Vec<PropertyChange> = bodies_of(ctx, scene, &self.i)?
            .into_iter()
            .filter_map(|id| scene.get(id))
            .filter(|body| body.optics != new)
            .map(|body| PropertyChange {
                id: body.id,
                old: PropertyValue::Optics(body.optics),
                new: PropertyValue::Optics(new),
            })
            .collect();
        if changes.is_empty() {
            return Ok(OpOutput::none());
        }
        Ok(OpOutput::none().message(PropertyEditIntent { changes }))
    }
}
