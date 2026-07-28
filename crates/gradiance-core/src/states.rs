//! Application and tool state machines.

use bevy::prelude::*;

/// Top-level simulation state.
///
/// Pausing is a state transition; the physics seam reacts to it by pausing
/// the physics clock. Authored entities are *not* state-scoped — they live
/// until deleted by a command.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    /// Simulation advancing.
    #[default]
    Playing,
    /// Simulation frozen; editing remains fully available.
    Paused,
}

/// The active editor tool.
///
/// Each tool is its own plugin whose systems/observers are gated on
/// `in_state(ToolState::X)`. Exactly one tool is active at a time.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum ToolState {
    /// Select, move, rotate, and duplicate bodies.
    #[default]
    Select,
    /// Grab and fling bodies with a physical constraint.
    Drag,
    /// Drag out a rectangle. A fast path for a sketch that happens to be one
    /// box: the body it makes carries the four constrained lines, so dragging
    /// a corner afterwards keeps it rectangular.
    Box,
    /// Drag from centre to rim. The radius stays a solver parameter, so a
    /// later diameter dimension can drive it.
    Circle,
    /// Chain line segments, inferring axis constraints as you draw. Closing
    /// the loop gives you a polygon; leaving it open gives you a link.
    Line,
    /// Sweep an arc from a centre through a start and end point.
    Arc,
    /// Trim geometry back to a boundary — or extend it forward to one, which
    /// is the same gesture.
    Trim,
    /// Connect two bodies (or one body to the world) with a revolute joint.
    Hinge,
    /// Rigidly weld two bodies together.
    Weld,
    /// Connect two bodies with a prismatic (slider) joint.
    Slider,
    /// Connect two points with a spring-damper strut (drag from one anchor
    /// to the other; the drag length sets the rest length).
    Strut,
    /// Draw static ground planes.
    Ground,
    /// Cut bodies along a dragged segment (CSG difference).
    Cut,
    /// Place tracer nodes (a placeable trajectory probe; attaches to a body
    /// under the cursor, else free).
    Tracer,
}
