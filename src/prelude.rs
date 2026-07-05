//! Convenience re-exports for internal modules and tests.

pub use crate::command::array_cmd::ArrayMode;
pub use crate::command::intent::{
    ArrayIntent, CommitTransformIntent, DeleteIntent, DuplicateIntent, GroupIntent,
    PropertyEditIntent, RedoIntent, ScaleIntent, SpawnBodyIntent, SpawnJointIntent,
    TransformChange, UndoIntent, UngroupIntent,
};
pub use crate::command::property::{PropertyChange, PropertyValue};
pub use crate::command::snapshot::{BodyRecord, JointRecord};
// Note: `CommandStack` is deliberately NOT re-exported — only the dispatcher
// (and tests, via the full `crate::command::CommandStack` path) may name it.
pub use crate::GradiancePlugins;
pub use crate::command::{CommandDispatchSet, CommandError, CommandPlugin, GameCommand};
pub use crate::core::CorePlugin;
pub use crate::core::constants::*;
pub use crate::core::ids::{IdIndex, StableId};
pub use crate::core::states::{GameState, ToolState};
pub use crate::core::units::PosRot;
pub use crate::domain::appearance::{Appearance, Rgba};
pub use crate::domain::group::SelectionGroup;
pub use crate::domain::joint::{JointCommon, JointDef, JointKind, MotorDef};
pub use crate::domain::layers::LayerMask32;
pub use crate::domain::props::{BodyKind, PhysicalProps};
pub use crate::domain::settings::{GridSettings, GridSystem, SnapConfig, SnapSources};
pub use crate::domain::shape::{ShapeDef, ShapeError};
pub use crate::domain::{Body, DomainPlugin, Joint};
pub use crate::interaction::cursor::CursorWorldPos;
pub use crate::interaction::gesture::{AxisConstraint, GestureConstraints};
pub use crate::interaction::selection::Selection;
pub use crate::interaction::snap::{SnapKind, SnappedCursor};
pub use crate::interaction::tools::ActiveGesture;
pub use crate::interaction::tools::handles::ScaleFrame;
pub use crate::interaction::{InteractionPlugin, PointerOverUi};
