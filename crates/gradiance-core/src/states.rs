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
    /// Draw axis-aligned rectangles.
    Box,
    /// Draw circles.
    Circle,
    /// Click out arbitrary polygons.
    Polygon,
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

/// Which authoring surface is active.
///
/// Sketch mode is **additive**: it does not replace or reimplement any of the
/// [`ToolState`] tools. Those are gated to [`EditorMode::Direct`], so they keep
/// working exactly as before and are simply inert while sketching — the
/// separation is mechanical rather than a matter of care.
///
/// Bodies drawn by the direct tools are conceptually unconstrained sketches
/// with a creation shortcut, but they carry no sketch document; only bodies
/// authored in [`EditorMode::Sketch`] retain one.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum EditorMode {
    /// Direct manipulation: the twelve `ToolState` tools.
    #[default]
    Direct,
    /// Constrained sketching, solved by SolveSpace.
    ///
    /// The simulation is paused on entry — solving geometry against a running
    /// sim is meaningless — and the previous run state is restored on exit.
    Sketch,
}

/// The active tool *within* sketch mode.
///
/// Gated on [`EditorMode::Sketch`]; meaningless in [`EditorMode::Direct`].
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Reflect)]
pub enum SketchTool {
    /// Select, drag and dimension existing sketch geometry, re-solving live.
    ///
    /// Constraining is *not* a tool: the applicable constraints follow from
    /// whatever is selected, so the panel offers them from any tool rather
    /// than making the author switch modes to say "and these are parallel".
    #[default]
    Select,
    /// Chain line segments, inferring constraints as you draw.
    Line,
    /// Sweep an arc from a centre through a start and end point.
    Arc,
    /// Place circles.
    Circle,
    /// Trim geometry back to a boundary — or extend it forward to one, which
    /// is the same gesture.
    Trim,
}
