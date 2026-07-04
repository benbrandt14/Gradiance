//! Intent messages — the only doorway from input/UI to world mutation.
//!
//! Tools and UI construct fully-specified intents (records built, ids
//! resolved, old/new values captured) and write them with a
//! `MessageWriter`. The dispatcher turns each into a [`GameCommand`]
//! (one intent, one undo step).

use crate::command::snapshot::BodyRecord;
use crate::core::ids::StableId;
use crate::core::units::PosRot;
use bevy::prelude::*;

/// Request to spawn one fully-specified body.
#[derive(Message, Debug, Clone)]
pub struct SpawnBodyIntent {
    /// The complete authored state to create.
    pub record: BodyRecord,
}

/// Request to delete a set of bodies.
#[derive(Message, Debug, Clone)]
pub struct DeleteIntent {
    /// Bodies to delete.
    pub targets: Vec<StableId>,
}

/// Request to duplicate a set of bodies at an offset.
#[derive(Message, Debug, Clone)]
pub struct DuplicateIntent {
    /// Bodies to clone.
    pub sources: Vec<StableId>,
    /// World-space offset applied to each clone.
    pub offset: Vec2,
}

/// One body's pose change within a [`CommitTransformIntent`].
#[derive(Debug, Clone, Copy)]
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
#[derive(Message, Debug, Clone)]
pub struct CommitTransformIntent {
    /// Every body's old and new pose.
    pub changes: Vec<TransformChange>,
}

/// Request to undo the last command.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct UndoIntent;

/// Request to redo the last undone command.
#[derive(Message, Debug, Clone, Copy, Default)]
pub struct RedoIntent;
