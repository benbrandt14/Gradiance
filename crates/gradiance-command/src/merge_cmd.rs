//! Booleans between bodies: merge (union), subtract, intersect and xor.
//!
//! Merge is the SDF answer to "weld should make two objects into one
//! object"; subtract / intersect / xor are the CSG modeling operations of
//! the context menu (roadmap M21, feedback 7.3). All four are one command,
//! [`MergeCommand`], differing only in the [`CsgOp`] that folds the
//! operands together.
//!
//! **Host and operands.** The first target is the **host**: every other
//! body's shape is placed into the host's frame and folded onto its tree
//! left to right (`((host op b) op c) …`), and the host keeps its id,
//! pose, props, and group. Every other operand is **consumed** — it
//! despawns, as the tool body does in CAD. The rule is the same for every
//! op and every surface (menu, verb, weld tool); undo restores them all.
//!
//! **Connectivity.** A union (or smooth union) must be a single connected
//! component — merging disjoint bodies would be a phantom rigid link,
//! which is what grouping is for — so it is refused (`NoEffect`)
//! otherwise. Subtract, intersect and xor may legitimately split the
//! host (a bar subtracted across a box, the two private halves of an
//! xor): the host is then replaced by polygon-leaf pieces exactly as a
//! severing cut replaces a body (`cut_cmd::piece_records`). An empty
//! result is refused rather than deleting everything, and so is a
//! non-union op whose operand misses the host's bounds (it would only
//! delete the operand).
//!
//! **Joints.** Joints *between* targets become internal and are deleted.
//! A joint end on a consumed body re-anchors onto the host at the same
//! world point. A joint end whose anchor sat in material the op removed
//! is deleted, as a cut deletes joints whose material is destroyed (a
//! union never removes material, so merge never does this). When the
//! host splits, each end moves to the piece containing its anchor, or
//! the joint is deleted. Trees deeper than [`MAX_CSG_DEPTH`] bake to a
//! polygon leaf.

use crate::cut_cmd::{
    aabbs_overlap, apply_joint_changes, piece_records, replace_with_pieces, shape_contains,
};
use crate::{CommandError, GameCommand, resolve};
use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_domain::joint::JointDef;
use gradiance_domain::shape::{CsgOp, MAX_CSG_DEPTH, ShapeDef};
use gradiance_geometry::polygonize::polygonize_components;
use gradiance_geometry::sdf;
use gradiance_scene::{BodyRecord, JointRecord};

/// How far outside a shape (metres of field) an anchor may sit and still
/// count as on its material — absorbs contouring and placement round-off.
const ANCHOR_TOLERANCE: f32 = 1e-3;

/// What the boolean does to the host.
#[derive(Debug)]
enum Outcome {
    /// One connected result: the host's shape becomes it.
    Reshape(ShapeDef),
    /// A disconnected result: the host is replaced by these pieces.
    Split(Vec<BodyRecord>),
}

/// Everything staged for one boolean (computed once, replayed on redo).
#[derive(Debug)]
struct Staged {
    /// Pre-boolean records: host first, then the consumed operands.
    originals: Vec<BodyRecord>,
    /// What happens to the host.
    outcome: Outcome,
    /// Joint rewires: old record, and the rewired definition
    /// (`None` = the joint is deleted).
    joint_changes: Vec<(JointRecord, Option<JointDef>)>,
}

/// What happens to one joint end.
enum EndFate {
    /// The end is not on a target, or nothing about it changes.
    Keep,
    /// The end moves to `body` at `anchor` (that body's frame), and its
    /// rest rotation becomes `rest_rot` when given.
    Move {
        body: StableId,
        anchor: Vec2,
        rest_rot: Option<f32>,
    },
    /// The end's material is gone: the joint is deleted.
    Drop,
}

/// Combines two or more bodies with a CSG boolean into the first one.
#[derive(Debug)]
pub struct MergeCommand {
    /// The boolean folding the operands onto the host.
    pub op: CsgOp,
    /// Bodies to combine; the first is the host, the rest are consumed.
    pub targets: Vec<StableId>,
}

impl MergeCommand {
    /// Builds a merge (union) of `targets` into `targets[0]`.
    pub fn new(targets: Vec<StableId>) -> Self {
        Self::with_op(CsgOp::Union, targets)
    }

    /// Builds the boolean `op` of `targets`, folded onto `targets[0]`.
    pub fn with_op(op: CsgOp, targets: Vec<StableId>) -> Self {
        Self { op, targets }
    }

    /// Whether the op must leave one connected body (union-like ops do:
    /// they only add material, so a split means the operands never met).
    fn requires_connected(&self) -> bool {
        matches!(self.op, CsgOp::Union | CsgOp::SmoothUnion { .. })
    }

    fn stage(&mut self, world: &mut World) -> Result<Staged, CommandError> {
        if self.targets.len() < 2 {
            return Err(CommandError::NoEffect);
        }
        // A body cannot be both host and operand (or an operand twice).
        for (i, id) in self.targets.iter().enumerate() {
            if self.targets[..i].contains(id) {
                return Err(CommandError::NoEffect);
            }
        }
        let mut originals = Vec::with_capacity(self.targets.len());
        for &id in &self.targets {
            let entity = resolve(world, id)?;
            let record =
                BodyRecord::capture(world, entity).ok_or(CommandError::MissingEntity(id))?;
            if record.shape.contains_half_plane() {
                return Err(CommandError::NoEffect); // the ground is not an operand
            }
            originals.push(record);
        }
        let host = originals[0].clone();
        let host_inv = Vec2::from_angle(-host.pose.rot);

        // Fold every operand into the host's local frame.
        let mut tree = host.shape.clone();
        for operand in &originals[1..] {
            let placed = ShapeDef::Placed {
                pos: host_inv.rotate(operand.pose.pos - host.pose.pos),
                rot: operand.pose.rot - host.pose.rot,
                shape: Box::new(operand.shape.clone()),
            };
            // An operand that misses the host would only be deleted.
            if !self.requires_connected() && !aabbs_overlap(sdf::aabb(&tree), sdf::aabb(&placed)) {
                return Err(CommandError::NoEffect);
            }
            tree = ShapeDef::Csg {
                op: self.op,
                lhs: Box::new(tree),
                rhs: Box::new(placed),
            };
        }

        let mut components = polygonize_components(&tree);
        let outcome = match components.len() {
            // Nothing left: refused rather than deleting every operand.
            0 => return Err(CommandError::NoEffect),
            1 if tree.depth() > MAX_CSG_DEPTH => {
                let piece = components.remove(0);
                // Not recentered: the host origin (and its joints) stay put.
                Outcome::Reshape(ShapeDef::Polygon {
                    outline: piece.outline,
                    holes: piece.holes,
                })
            }
            1 => Outcome::Reshape(tree.clone()),
            // A union must produce one connected solid; use grouping for
            // rigid links between separated bodies.
            _ if self.requires_connected() => return Err(CommandError::NoEffect),
            _ => Outcome::Split(piece_records(&host, components)),
        };

        let joint_changes =
            boolean_joint_changes(world, &self.targets, &originals, &tree, &outcome);

        Ok(Staged {
            originals,
            outcome,
            joint_changes,
        })
    }
}

/// The joint rewires of one boolean: internal joints are deleted, ends on
/// consumed operands move onto the host (or onto the piece containing
/// them when the host splits), and ends whose material is gone delete
/// their joint. Joints that do not change are left out.
fn boolean_joint_changes(
    world: &mut World,
    targets: &[StableId],
    originals: &[BodyRecord],
    tree: &ShapeDef,
    outcome: &Outcome,
) -> Vec<(JointRecord, Option<JointDef>)> {
    let host = &originals[0];
    let host_inv = Vec2::from_angle(-host.pose.rot);
    // A target-relative anchor, re-expressed in the host frame.
    let to_host = |body: StableId, anchor: Vec2| -> Option<Vec2> {
        let source = originals.iter().find(|r| r.id == body)?;
        let world_point = source.pose.pos + Vec2::from_angle(source.pose.rot).rotate(anchor);
        Some(host_inv.rotate(world_point - host.pose.pos))
    };
    // Piece centroids in the host frame (empty unless split).
    let pieces: Vec<(&BodyRecord, Vec2)> = match outcome {
        Outcome::Split(pieces) => pieces
            .iter()
            .map(|p| (p, host_inv.rotate(p.pose.pos - host.pose.pos)))
            .collect(),
        Outcome::Reshape(_) => Vec::new(),
    };
    let fate = |body: StableId, anchor: Vec2| -> EndFate {
        let Some(source) = originals.iter().find(|r| r.id == body) else {
            return EndFate::Keep; // not a target
        };
        let Some(local) = to_host(body, anchor) else {
            return EndFate::Keep;
        };
        let consumed = body != host.id;
        if !pieces.is_empty() {
            return pieces
                .iter()
                .find(|(piece, offset)| shape_contains(&piece.shape, local - *offset))
                .map_or(EndFate::Drop, |(piece, offset)| EndFate::Move {
                    body: piece.id,
                    anchor: local - *offset,
                    rest_rot: consumed.then_some(host.pose.rot),
                });
        }
        let was_on_material = sdf::eval(&source.shape, anchor) <= ANCHOR_TOLERANCE;
        if was_on_material && sdf::eval(tree, local) > ANCHOR_TOLERANCE {
            EndFate::Drop
        } else if consumed {
            EndFate::Move {
                body: host.id,
                anchor: local,
                rest_rot: Some(host.pose.rot),
            }
        } else {
            EndFate::Keep
        }
    };

    let in_set = |id: StableId| targets.contains(&id);
    crate::joint_cmd::joints_referencing(world, targets)
        .into_iter()
        .filter_map(|(_, record)| {
            let def = &record.def;
            // A joint whose endpoints are both inside the set (world
            // pins are outside — the world is not an operand) becomes
            // internal and is deleted.
            let both_inside = in_set(def.body_a) && matches!(def.body_b, Some(b) if in_set(b));
            if both_inside {
                return Some((record, None));
            }
            let mut new = def.clone();
            match fate(def.body_a, def.anchor_a) {
                EndFate::Keep => {}
                EndFate::Drop => return Some((record, None)),
                EndFate::Move {
                    body,
                    anchor,
                    rest_rot,
                } => {
                    new.body_a = body;
                    new.anchor_a = anchor;
                    if let Some(rot) = rest_rot {
                        new.rest_rot_a = rot;
                    }
                }
            }
            if let Some(b) = def.body_b {
                match fate(b, def.anchor_b) {
                    EndFate::Keep => {}
                    EndFate::Drop => return Some((record, None)),
                    EndFate::Move {
                        body,
                        anchor,
                        rest_rot,
                    } => {
                        new.body_b = Some(body);
                        new.anchor_b = anchor;
                        if let Some(rot) = rest_rot {
                            new.rest_rot_b = rot;
                        }
                    }
                }
            }
            (new != *def).then_some((record, Some(new)))
        })
        .collect()
}

impl GameCommand for MergeCommand {
    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        let staged = self.stage(world)?;
        for consumed in &staged.originals[1..] {
            let entity = resolve(world, consumed.id)?;
            world.despawn(entity);
        }
        let host = &staged.originals[0];
        match &staged.outcome {
            Outcome::Reshape(shape) => {
                let entity = resolve(world, host.id)?;
                if let Some(mut live) = world.get_mut::<ShapeDef>(entity) {
                    *live = shape.clone();
                }
            }
            Outcome::Split(pieces) => replace_with_pieces(world, host, pieces)?,
        }
        apply_joint_changes(world, &staged.joint_changes)?;
        Ok(())
    }

    fn name(&self) -> &'static str {
        use crate::intent::name;
        match self.op {
            CsgOp::Union | CsgOp::SmoothUnion { .. } => name::MERGE,
            CsgOp::Subtract => name::SUBTRACT,
            CsgOp::Intersect => name::INTERSECT,
            CsgOp::Xor => name::XOR,
        }
    }
}
