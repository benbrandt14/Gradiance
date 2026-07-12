//! Merging bodies into one — the SDF answer to "weld should make two
//! objects into one object".
//!
//! The first target is the **host**: every other body's shape is placed
//! into the host's frame and `Union`-ed onto its tree, the absorbed
//! bodies despawn, and the host keeps its id, pose, props, and group.
//! The merge is refused (`NoEffect`) unless the union is a single
//! connected component — merging disjoint bodies would be a phantom
//! rigid link, which is what grouping is for.
//!
//! Joints on absorbed bodies re-anchor onto the host (same world point);
//! joints *between* merged bodies become internal and are deleted. Trees
//! deeper than [`MAX_CSG_DEPTH`] bake to a polygon leaf.

use crate::command::cut_cmd::{apply_joint_changes, revert_joint_changes};
use crate::command::snapshot::{BodyRecord, JointRecord};
use crate::command::{CommandError, GameCommand, resolve};
use crate::core::ids::StableId;
use crate::domain::joint::JointDef;
use crate::domain::shape::{CsgOp, MAX_CSG_DEPTH, ShapeDef};
use crate::geometry::polygonize::polygonize_components;
use bevy::prelude::*;

/// Everything staged for one merge (computed once, replayed on redo).
#[derive(Debug)]
struct Staged {
    /// Pre-merge records: host first, then the absorbed bodies.
    originals: Vec<BodyRecord>,
    /// The host's post-merge shape.
    merged_shape: ShapeDef,
    /// Joint rewires: old record, and the host-anchored definition
    /// (`None` = the joint was internal to the merge and is deleted).
    joint_changes: Vec<(JointRecord, Option<JointDef>)>,
}

/// Merges two or more bodies into the first one.
#[derive(Debug)]
pub struct MergeCommand {
    /// Bodies to merge; the first is the host that survives.
    pub targets: Vec<StableId>,
    staged: Option<Staged>,
}

impl MergeCommand {
    /// Builds a merge of `targets` into `targets[0]`.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self {
            targets,
            staged: None,
        }
    }

    fn stage(&mut self, world: &mut World) -> Result<Staged, CommandError> {
        if self.targets.len() < 2 {
            return Err(CommandError::NoEffect);
        }
        let mut originals = Vec::with_capacity(self.targets.len());
        for &id in &self.targets {
            let entity = resolve(world, id)?;
            let record =
                BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
            if record.shape.contains_half_plane() {
                return Err(CommandError::NoEffect); // the ground is not mergeable
            }
            originals.push(record);
        }
        let host = originals[0].clone();
        let host_inv = Vec2::from_angle(-host.pose.rot);

        // Union every absorbed body into the host's local frame.
        let mut tree = host.shape.clone();
        for absorbed in &originals[1..] {
            tree = ShapeDef::Csg {
                op: CsgOp::Union,
                lhs: Box::new(tree),
                rhs: Box::new(ShapeDef::Placed {
                    pos: host_inv.rotate(absorbed.pose.pos - host.pose.pos),
                    rot: absorbed.pose.rot - host.pose.rot,
                    shape: Box::new(absorbed.shape.clone()),
                }),
            };
        }

        // A merge must produce one connected solid; use grouping for
        // rigid links between separated bodies.
        let mut components = polygonize_components(&tree);
        if components.len() != 1 {
            return Err(CommandError::NoEffect);
        }
        let merged_shape = if tree.depth() > MAX_CSG_DEPTH {
            let piece = components.remove(0);
            // Not recentered: the host origin (and its joints) stay put.
            ShapeDef::Polygon {
                outline: piece.outline,
                holes: piece.holes,
            }
        } else {
            tree
        };

        // Rewire joints from absorbed bodies onto the host at the same
        // world point; joints internal to the merged set are deleted.
        let absorbed_ids: Vec<StableId> = self.targets[1..].to_vec();
        let in_set = |id: StableId| self.targets.contains(&id);
        let joint_changes = crate::command::joint_cmd::joints_referencing(world, &absorbed_ids)
            .into_iter()
            .map(|(_, record)| {
                let def = &record.def;
                // A joint whose endpoints are both inside the merged set
                // (world pins are outside — the world is not merged)
                // becomes internal and is deleted.
                let both_inside = in_set(def.body_a) && matches!(def.body_b, Some(b) if in_set(b));
                if both_inside {
                    return (record, None);
                }
                let mut new = def.clone();
                // Anchor's world point, re-expressed in the host frame.
                let rewire = |body: StableId, anchor: Vec2| -> Option<Vec2> {
                    let source = originals.iter().find(|r| r.id == body)?;
                    let world_point =
                        source.pose.pos + Vec2::from_angle(source.pose.rot).rotate(anchor);
                    Some(host_inv.rotate(world_point - host.pose.pos))
                };
                if absorbed_ids.contains(&new.body_a)
                    && let Some(anchor) = rewire(new.body_a, new.anchor_a)
                {
                    new.anchor_a = anchor;
                    new.body_a = host.id;
                    new.rest_rot_a = host.pose.rot;
                }
                if let Some(b) = new.body_b
                    && absorbed_ids.contains(&b)
                    && let Some(anchor) = rewire(b, new.anchor_b)
                {
                    new.anchor_b = anchor;
                    new.body_b = Some(host.id);
                    new.rest_rot_b = host.pose.rot;
                }
                (record, Some(new))
            })
            .collect();

        Ok(Staged {
            originals,
            merged_shape,
            joint_changes,
        })
    }
}

impl GameCommand for MergeCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        if self.staged.is_none() {
            self.staged = Some(self.stage(world)?);
        }
        let Some(staged) = &self.staged else {
            return Err(CommandError::NoEffect);
        };
        for absorbed in &staged.originals[1..] {
            let entity = resolve(world, absorbed.id)?;
            world.despawn(entity);
        }
        let host = resolve(world, staged.originals[0].id)?;
        if let Some(mut shape) = world.get_mut::<ShapeDef>(host) {
            *shape = staged.merged_shape.clone();
        }
        apply_joint_changes(world, &staged.joint_changes)?;
        Ok(())
    }

    fn undo(&mut self, world: &mut World) -> Result<(), CommandError> {
        let Some(staged) = &self.staged else {
            return Err(CommandError::NoEffect);
        };
        let host = resolve(world, staged.originals[0].id)?;
        if let Some(mut shape) = world.get_mut::<ShapeDef>(host) {
            *shape = staged.originals[0].shape.clone();
        }
        for absorbed in &staged.originals[1..] {
            absorbed.spawn(world);
        }
        revert_joint_changes(world, &staged.joint_changes)?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        crate::command::intent::name::MERGE
    }
}
