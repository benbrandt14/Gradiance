//! Intent messages — the only doorway from input/UI to world mutation.
//!
//! Tools and UI construct fully-specified intents (records built, ids
//! resolved, old/new values captured) and write them with a
//! `MessageWriter`. The dispatcher turns each into a
//! [`GameCommand`](crate::command::GameCommand) (one intent, one undo step).

use crate::command::snapshot::BodyRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use bevy::prelude::*;

/// Request to spawn one fully-specified body.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnBodyIntent {
    /// The complete authored state to create.
    pub record: BodyRecord,
}

/// Request to delete a set of bodies.
#[derive(Message, Debug, Clone, Reflect)]
pub struct DeleteIntent {
    /// Bodies to delete.
    pub targets: Vec<StableId>,
}

/// Request to duplicate a set of bodies at an offset.
#[derive(Message, Debug, Clone, Reflect)]
pub struct DuplicateIntent {
    /// Bodies to clone.
    pub sources: Vec<StableId>,
    /// World-space offset applied to each clone.
    pub offset: Vec2,
}

/// One body's pose change within a [`CommitTransformIntent`].
#[derive(Debug, Clone, Copy, Reflect)]
pub struct TransformChange {
    /// The body that moved.
    pub id: StableId,
    /// Pose when the gesture started.
    pub old: PosRot,
    /// Pose when the gesture ended.
    pub new: PosRot,
}

/// Commit of a completed move/rotate gesture (one undo step for the whole
/// gesture, however many bodies it touched).
#[derive(Message, Debug, Clone, Reflect)]
pub struct CommitTransformIntent {
    /// Every body's old and new pose.
    pub changes: Vec<TransformChange>,
}

/// Commit of a completed scale gesture (or a numeric scale edit).
#[derive(Message, Debug, Clone, Reflect)]
pub struct ScaleIntent {
    /// Bodies to scale.
    pub targets: Vec<StableId>,
    /// Fixed point, world space.
    pub pivot: Vec2,
    /// Frame rotation: 0 = global axes, body rotation = local axes.
    pub frame_rot: f32,
    /// Per-axis factors along the frame axes.
    pub factors: Vec2,
}

/// Request to pattern-copy bodies (linear or radial array).
#[derive(Message, Debug, Clone, Reflect)]
pub struct ArrayIntent {
    /// Bodies to pattern.
    pub sources: Vec<StableId>,
    /// Number of copies.
    pub count: u32,
    /// Placement rule.
    pub mode: crate::command::array_cmd::ArrayMode,
}

/// Request to create one fully-specified joint.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnJointIntent {
    /// The complete authored joint (id minted by the tool, stable across
    /// redo).
    pub record: crate::command::snapshot::JointRecord,
}

/// Batched property edit (one gesture across N targets = one undo step).
#[derive(Message, Debug, Clone, Reflect)]
pub struct PropertyEditIntent {
    /// Old → new value per target.
    pub changes: Vec<crate::command::property::PropertyChange>,
}

/// Request to group bodies together.
#[derive(Message, Debug, Clone, Reflect)]
pub struct GroupIntent {
    /// Bodies to group.
    pub targets: Vec<StableId>,
}

/// Request to remove bodies from their groups.
#[derive(Message, Debug, Clone, Reflect)]
pub struct UngroupIntent {
    /// Bodies to ungroup.
    pub targets: Vec<StableId>,
}

/// Request to replace the whole world with a scene (undoable).
#[derive(Message, Debug, Clone, Reflect)]
pub struct LoadSceneIntent {
    /// The parsed scene to load.
    pub scene: crate::command::snapshot::SceneRecord,
}

/// Request to delete one joint (undoable, restores the same id).
#[derive(Message, Debug, Clone, Reflect)]
pub struct DeleteJointIntent {
    /// The joint to delete.
    pub id: StableId,
}

/// Request to cut every body crossed by a stroke (CSG subtract; severed
/// bodies split into pieces).
///
/// Its fields are all leaf-reflectable (`Vec2`/`f32`); it was the first
/// intent to derive `Reflect`. The rest of the authored intent surface
/// (`SpawnBodyIntent`, `SpawnJointIntent`, …) now derives `Reflect` too, once
/// spike #1 settled `StableId`/`ShapeDef` reflect-opacity — see
/// `docs/script-spike-findings.md`.
#[derive(Message, Debug, Clone, Reflect)]
pub struct CutIntent {
    /// Stroke start, world space.
    pub a: Vec2,
    /// Stroke end, world space.
    pub b: Vec2,
    /// Stroke width, world pixels.
    pub width: f32,
}

/// Request to merge bodies into one (SDF union; first target hosts).
#[derive(Message, Debug, Clone, Reflect)]
pub struct MergeIntent {
    /// Bodies to merge; the first survives with the union of all shapes.
    pub targets: Vec<StableId>,
}

/// Request to undo the last command.
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct UndoIntent;

/// Request to redo the last undone command.
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct RedoIntent;
