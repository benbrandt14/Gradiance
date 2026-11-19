//! Intent messages — the only doorway from input/UI to world mutation.
//!
//! Tools and UI construct fully-specified intents (records built, ids
//! resolved, old/new values captured) and write them with a
//! `MessageWriter`. The dispatcher turns each into a
//! [`GameCommand`](crate::GameCommand) (one intent, one undo step).
//!
//! Each intent carries a `Trace:` line (dispatch site → command → sync
//! systems); grep its [`name`] constant to walk the whole chain.

use bevy::prelude::*;
use gradiance_core::ids::StableId;
use gradiance_core::units::PosRot;
use gradiance_scene::BodyRecord;

/// Canonical kebab-case names, one per intent/command pair — shared by
/// [`GameCommand::name`](crate::GameCommand::name) and (where a
/// script verb maps 1:1, like [`CUT`](name::CUT)) the operation registry,
/// so one grep walks the whole chain.
pub mod name {
    /// [`SpawnBodyIntent`](super::SpawnBodyIntent) → `SpawnCommand<BodyRecord>`.
    pub const SPAWN_BODY: &str = "spawn-body";
    /// [`SpawnNodeIntent`](super::SpawnNodeIntent) → `SpawnCommand<NodeRecord>`.
    pub const SPAWN_NODE: &str = "spawn-node";
    /// [`DeleteIntent`](super::DeleteIntent) → `DeleteCommand`.
    pub const DELETE: &str = "delete";
    /// [`DuplicateIntent`](super::DuplicateIntent) → `DuplicateCommand`.
    pub const DUPLICATE: &str = "duplicate";
    /// [`CommitTransformIntent`](super::CommitTransformIntent) → `CommitTransformCommand`.
    pub const COMMIT_TRANSFORM: &str = "commit-transform";
    /// [`ScaleIntent`](super::ScaleIntent) → `ScaleCommand`.
    pub const SCALE: &str = "scale";
    /// [`ArrayIntent`](super::ArrayIntent) → `ArrayCommand`.
    pub const ARRAY: &str = "array";
    /// [`SpawnJointIntent`](super::SpawnJointIntent) → `SpawnJointCommand`.
    pub const SPAWN_JOINT: &str = "spawn-joint";
    /// [`SpawnRodIntent`](super::SpawnRodIntent) → `SpawnRodCommand`.
    pub const SPAWN_ROD: &str = "spawn-rod";
    /// [`PropertyEditIntent`](super::PropertyEditIntent) → `PropertyEditCommand`.
    pub const PROPERTY_EDIT: &str = "property-edit";
    /// [`GroupIntent`](super::GroupIntent) → `GroupCommand`.
    pub const GROUP: &str = "group";
    /// [`UngroupIntent`](super::UngroupIntent) → `UngroupCommand`.
    pub const UNGROUP: &str = "ungroup";
    /// [`NameGroupIntent`](super::NameGroupIntent) → `NameGroupCommand`.
    pub const NAME_GROUP: &str = "name-group";
    /// [`RegroupIntent`](super::RegroupIntent) → `RegroupCommand` → script
    /// verb `(decompose …)`.
    pub const REGROUP: &str = "regroup";
    /// [`CutIntent`](super::CutIntent) → `CutCommand` → script verb `(cut …)`.
    pub const CUT: &str = "cut";
    /// [`DeleteJointIntent`](super::DeleteJointIntent) → `DeleteJointCommand`.
    pub const DELETE_JOINT: &str = "delete-joint";
    /// [`MergeIntent`](super::MergeIntent) → `MergeCommand`.
    pub const MERGE: &str = "merge";
    /// [`BooleanIntent`](super::BooleanIntent) with `CsgOp::Subtract`.
    pub const SUBTRACT: &str = "subtract";
    /// [`BooleanIntent`](super::BooleanIntent) with `CsgOp::Intersect`.
    pub const INTERSECT: &str = "intersect";
    /// [`BooleanIntent`](super::BooleanIntent) with `CsgOp::Xor`.
    pub const XOR: &str = "xor";
    /// [`BooleanIntent`](super::BooleanIntent) with `CsgOp::SmoothUnion`
    /// (a merge with a fillet) → script verb `(fillet-merge …)`.
    pub const FILLET_MERGE: &str = "fillet-merge";
    /// [`LoadSceneIntent`](super::LoadSceneIntent) → `LoadSceneCommand`.
    pub const LOAD_SCENE: &str = "load-scene";
    /// [`UndoIntent`](super::UndoIntent) → `CommandStack::undo`.
    pub const UNDO: &str = "undo";
    /// [`RedoIntent`](super::RedoIntent) → `CommandStack::redo`.
    pub const REDO: &str = "redo";
    /// A simulation run, recorded as one undo step at a play/pause boundary.
    pub const SIMULATE: &str = "simulate";
    /// A settled edit to the scene-content settings (gravity, render style,
    /// lighting, scenery), recorded as one undo step.
    pub const SETTINGS: &str = "settings";
}

/// Request to spawn one fully-specified body.
// Trace: dispatch.rs → SpawnCommand (spawn.rs) → sync_colliders, sync_collision_layers, sync_body_meshes, sync_body_materials.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnBodyIntent {
    /// The complete authored state to create.
    pub record: BodyRecord,
}

/// Request to spawn one behavior node (a placeable tracer/sensor/actuator).
// Trace: dispatch.rs → SpawnCommand (spawn.rs) → follow_node_attachments, render::tracer.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnNodeIntent {
    /// The complete authored node state to create.
    pub record: gradiance_scene::NodeRecord,
}

/// Request to delete a set of bodies.
// Trace: dispatch.rs → DeleteCommand (spawn.rs) → despawn (avian/render derived state despawns with the entity); dangling joints → guard_dangling_joints.
#[derive(Message, Debug, Clone, Reflect)]
pub struct DeleteIntent {
    /// Bodies to delete.
    pub targets: Vec<StableId>,
}

/// Request to duplicate a set of bodies at an offset.
// Trace: dispatch.rs → DuplicateCommand (spawn.rs) → sync_colliders, sync_collision_layers, sync_body_meshes, sync_body_materials.
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
// Trace: dispatch.rs → CommitTransformCommand (transform_cmd.rs) → writes Transform directly (avian teleports; no Changed<> sync involved).
#[derive(Message, Debug, Clone, Reflect)]
pub struct CommitTransformIntent {
    /// Every body's old and new pose.
    pub changes: Vec<TransformChange>,
}

/// Commit of a completed scale gesture (or a numeric scale edit).
// Trace: dispatch.rs → ScaleCommand (scale_cmd.rs) → sync_colliders, sync_body_meshes (ShapeDef) + Transform writes.
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

/// Request to pattern-copy bodies (a row, a column, or a grid).
// Trace: dispatch.rs → ArrayCommand (array_cmd.rs) → sync_colliders, sync_collision_layers, sync_body_meshes, sync_body_materials.
#[derive(Message, Debug, Clone, Reflect)]
pub struct ArrayIntent {
    /// Bodies to pattern.
    pub sources: Vec<StableId>,
    /// Number of copies along the pattern's primary axis.
    pub count: u32,
    /// Placement rule.
    pub mode: crate::array_cmd::ArrayMode,
}

/// Request to create one fully-specified joint.
// Trace: dispatch.rs → SpawnJointCommand (joint_cmd.rs) → sync_joints.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnJointIntent {
    /// The complete authored joint (id minted by the tool, stable across
    /// redo).
    pub record: gradiance_scene::JointRecord,
}

/// Request to author a rigid rod: its capsule body and the hinges at the
/// ends that landed on bodies, as one undo step.
// Trace: dispatch.rs → SpawnRodCommand (rod_cmd.rs) → sync_colliders, sync_body_meshes, sync_joints.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SpawnRodIntent {
    /// The rod body (a `ShapeDef::Capsule`; id minted by the tool).
    pub body: BodyRecord,
    /// End joints referencing the rod body (zero, one, or two).
    pub joints: Vec<gradiance_scene::JointRecord>,
}

/// Batched property edit (one gesture across N targets = one undo step).
// Trace: dispatch.rs → PropertyEditCommand (property.rs) → whichever sync matches the edited component (sync_colliders/sync_body_meshes for Shape, sync_collision_layers/sync_body_meshes for Layers, sync_body_materials for Appearance, sync_joints for Joint; avian components apply directly).
#[derive(Message, Debug, Clone, Reflect)]
pub struct PropertyEditIntent {
    /// Old → new value per target.
    pub changes: Vec<crate::property::PropertyChange>,
}

/// Request to group bodies together.
// Trace: dispatch.rs → GroupCommand (group_cmd.rs) → writes SelectionGroup only (editor grouping; no derived sync).
#[derive(Message, Debug, Clone, Reflect)]
pub struct GroupIntent {
    /// Bodies to group.
    pub targets: Vec<StableId>,
}

/// Request to remove bodies from their groups.
// Trace: dispatch.rs → UngroupCommand (group_cmd.rs) → removes SelectionGroup only (no derived sync).
#[derive(Message, Debug, Clone, Reflect)]
pub struct UngroupIntent {
    /// Bodies to ungroup.
    pub targets: Vec<StableId>,
}

/// Request to name a group (an empty name removes the name).
// Trace: dispatch.rs → NameGroupCommand (group_cmd.rs) → writes the GroupNames scene-content resource only (no derived sync).
#[derive(Message, Debug, Clone, Reflect)]
pub struct NameGroupIntent {
    /// The group's id (the value in its members' `SelectionGroup` stacks).
    pub group: u32,
    /// The new name.
    pub name: String,
}

/// Request to split one group into parts, in one undo step: each part of
/// two or more bodies becomes a fresh group in the old group's place in
/// every member's stack (nesting is kept); a part of one body, or a member
/// in no part, just leaves the group.
// Trace: dispatch.rs → RegroupCommand (group_cmd.rs) → writes SelectionGroup and GroupNames only (no derived sync).
#[derive(Message, Debug, Clone, Reflect)]
pub struct RegroupIntent {
    /// The group's id.
    pub group: u32,
    /// The parts, each a list of the group's members.
    pub parts: Vec<Vec<StableId>>,
}

/// Request to replace the whole world with a scene (undoable).
// Trace: dispatch.rs → LoadSceneCommand (scene_cmd.rs) → full respawn → every sync system (colliders, layers, meshes, materials, joints).
#[derive(Message, Debug, Clone, Reflect)]
pub struct LoadSceneIntent {
    /// The parsed scene to load.
    pub scene: gradiance_scene::SceneRecord,
}

/// Request to delete one joint (undoable, restores the same id).
// Trace: dispatch.rs → DeleteJointCommand (joint_cmd.rs) → despawn (derived avian joint despawns with the entity).
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
// Trace: dispatch.rs → CutCommand (cut_cmd.rs) → sync_colliders, sync_body_meshes (rewritten ShapeDefs + spawned pieces).
#[derive(Message, Debug, Clone, Reflect)]
pub struct CutIntent {
    /// Stroke start, world space.
    pub a: Vec2,
    /// Stroke end, world space.
    pub b: Vec2,
    /// Stroke width, world metres.
    pub width: f32,
}

/// Request to merge bodies into one (SDF union; first target hosts).
// Trace: dispatch.rs → MergeCommand (merge_cmd.rs) → sync_colliders, sync_body_meshes (host ShapeDef union; other targets despawn).
#[derive(Message, Debug, Clone, Reflect)]
pub struct MergeIntent {
    /// Bodies to merge; the first survives with the union of all shapes.
    pub targets: Vec<StableId>,
}

/// Request to combine bodies with a CSG boolean — subtract, intersect,
/// xor, or a filleted merge (`SmoothUnion { radius }`; a plain union is
/// what [`MergeIntent`] means). The first target hosts the result; every
/// other target is consumed. An op with an invalid parameter (a smooth
/// union's radius not finite and positive) is refused and records nothing.
// Trace: dispatch.rs → MergeCommand (merge_cmd.rs) → sync_colliders, sync_body_meshes (host ShapeDef tree, or polygon pieces when the result splits; other targets despawn).
#[derive(Message, Debug, Clone, Reflect)]
pub struct BooleanIntent {
    /// The boolean folding each operand onto the host, left to right.
    pub op: gradiance_domain::shape::CsgOp,
    /// Bodies to combine; the first is the host.
    pub targets: Vec<StableId>,
}

/// Request to **discard** the whole undo/redo history, after this frame's
/// commands have applied: the world stays as it is and becomes the new
/// baseline. Review mode sends it after loading a question's scene, so the
/// demonstration never leaves undo steps behind (brief 4b-0). Not an op —
/// no script or menu discards history.
// Trace: dispatch.rs → `CommandStack::default()` after `execute`.
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct ClearHistoryIntent;

/// Request to adjust the last operation (brief U-11): undo its step and
/// invoke it again with `params`, recording one step in its place. Ignored
/// unless the history is still at `step` and the last operation is `id`.
/// Written by the `edit.adjust_last` operation only.
// Trace: dispatch.rs → CommandStack::undo → gradiance_op::invoke_as → the operation's intent → its command.
#[derive(Message, Debug, Clone)]
pub struct AdjustLastIntent {
    /// The operation's id.
    pub id: &'static str,
    /// The whole params object, with the adjusted field.
    pub params: gradiance_op::Value,
    /// The history revision the adjustment was made against.
    pub step: u64,
}

/// Request to undo the last command.
// Trace: dispatch.rs → CommandStack::undo (mod.rs).
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct UndoIntent;

/// Request to redo the last undone command.
// Trace: dispatch.rs → CommandStack::redo (mod.rs).
#[derive(Message, Debug, Clone, Copy, Default, Reflect)]
pub struct RedoIntent;
