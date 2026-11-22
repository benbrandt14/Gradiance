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
    /// Draw spur gears (brief S-5): press at the centre, drag out the pitch
    /// radius; every drawn gear has one tooth size, so any two mesh.
    Gear,
    /// Fill a dragged rectangle with particle water (brief S-11).
    Water,
    /// Place a body's image (brief S-13): drag moves, Shift-drag scales,
    /// right-drag turns.
    Texture,
    /// Draw freehand (brief N-6): the dragged stroke becomes one body, its
    /// segments joined as one union, or a filled polygon when it closes.
    Sketch,
    /// Connect two bodies (or one body to the world) with a revolute joint.
    Hinge,
    /// Rigidly weld two bodies together.
    Weld,
    /// Connect two bodies with a prismatic (slider) joint.
    Slider,
    /// Connect two points with a spring-damper (drag from one anchor to the
    /// other; the drag length sets the rest length). Formerly "Strut".
    Spring,
    /// Draw a rigid rod (a capsule body) between two points; each end that
    /// lands on a body is hinged to it.
    Rod,
    /// Draw a chain (brief N-5): drag a path; release lays capsule links
    /// along it, hinged end to end, its ends pinned per the tool defaults.
    Chain,
    /// Draw static ground planes.
    Ground,
    /// Cut bodies along a dragged segment (CSG difference).
    Cut,
    /// Place tracer nodes (a placeable trajectory probe; attaches to a body
    /// under the cursor, else free).
    Tracer,
    /// Place thruster nodes (a push at a point of a body; brief N-3). Press
    /// where it pushes; a drag aims it.
    Thruster,
    /// Place laser nodes (a reflecting beam with a hit signal; brief N-8).
    /// Press where it sits; a drag aims it; releasing over another body
    /// makes that body its target.
    Laser,
    /// A tool a script registered (`register-tool`): the index of its entry in
    /// [`ScriptTools`](crate::script_tools::ScriptTools). It collects clicks
    /// like the polygon tool and, on the last one, runs the script's body once.
    Script(u16),
}

impl ToolState {
    /// Whether this is a script-registered tool.
    #[must_use]
    pub fn is_script(self) -> bool {
        matches!(self, Self::Script(_))
    }
}
