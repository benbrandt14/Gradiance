//! The script tool: a generic, Rust-side [`DraftTool`] behind every tool a
//! `.scm` file registers with `(register-tool name points src)`.
//!
//! The scripting VM must never run per frame (the two-tier rule), so a
//! scripted tool is **not** a lisp closure called from `update`/`preview`.
//! This draft does the per-frame work itself, with the same driver, snapping
//! and [`SnapReference`](crate::snap::SnapReference) publishing as the
//! built-ins: each press adds a point, the preview draws the points and the
//! polyline through them, and Escape or a right-click cancels. When the last
//! point lands, it commits [`ToolCommit::Script`], which the driver sends as a
//! [`ScriptToolFired`](gradiance_core::script_tools::ScriptToolFired) message;
//! the script layer runs the tool's body **once**, with the points as that
//! run's context. One gesture = one run = one undo batch.
//!
//! Which tool is active comes from `ToolState::Script(i)` and the
//! [`ScriptTools`] table the script layer publishes — never from a dependency
//! on `gradiance-script` (the layer graph forbids it).

use crate::snap::DraftReference;
use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_core::script_tools::ScriptTools;
use gradiance_core::states::ToolState;

/// Radius of the marker drawn at each collected point, world units.
const POINT_MARK: f32 = 0.05;

/// The script tool's draft: which registered tool is active, and the points
/// collected so far (world space).
#[derive(Resource, Default, Debug)]
pub struct ScriptTool {
    /// The active tool's index and how many points it collects; `None` when
    /// the selected index names no registered tool.
    active: Option<(u16, usize)>,
    /// Points placed so far, in click order.
    points: Vec<Vec2>,
}

impl ScriptTool {
    /// Selects tool `tool`, which collects `needed` points. A different tool
    /// (or a changed point count, after a hot reload) drops the draft.
    pub fn select(&mut self, active: Option<(u16, usize)>) {
        if self.active != active {
            self.active = active;
            self.points.clear();
        }
    }

    /// The points collected so far.
    #[must_use]
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }
}

impl DraftTool for ScriptTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        let (tool, needed) = self.active?;
        if ctx.cancel || ctx.secondary == GesturePhase::Pressed {
            self.points.clear();
            return None;
        }
        if ctx.phase == GesturePhase::Pressed
            && let Some(p) = ctx.cursor
        {
            self.points.push(p);
            if self.points.len() >= needed {
                let points = std::mem::take(&mut self.points);
                return Some(ToolCommit::Script { tool, points });
            }
        }
        None
    }

    fn drafting(&self) -> bool {
        !self.points.is_empty()
    }

    fn snap_reference(&self) -> Option<DraftReference> {
        self.points.last().copied().map(DraftReference::Line)
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if self.points.is_empty() {
            return;
        }
        for p in &self.points {
            out.circle(*p, POINT_MARK, css::GOLD);
        }
        if self.points.len() > 1 {
            out.polyline(self.points.clone(), css::AQUAMARINE);
        }
        if let (Some(last), Some(p)) = (self.points.last().copied(), ctx.cursor) {
            out.line(last, p, css::AQUAMARINE.with_alpha(0.5));
        }
    }
}

/// Run condition: a script tool is the active tool.
pub fn script_tool_active(state: Res<State<ToolState>>) -> bool {
    state.get().is_script()
}

/// Points the draft at the tool `ToolState::Script(i)` names, reading the
/// published table. Runs before the driver each frame; cheap (a lookup), and
/// it writes the draft only when the selection actually changes.
pub fn sync_script_tool(
    state: Res<State<ToolState>>,
    tools: Res<ScriptTools>,
    mut draft: ResMut<ScriptTool>,
) {
    let active = match *state.get() {
        ToolState::Script(i) => tools.get(i).map(|t| (i, t.points.max(1))),
        _ => None,
    };
    if draft.active != active {
        draft.select(active);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::GestureConstraints;
    use gradiance_domain::settings::{SnapConfig, ToolDefaults};

    fn ctx<'a>(
        phase: GesturePhase,
        secondary: GesturePhase,
        cursor: Vec2,
        constraints: &'a GestureConstraints,
        snap: &'a SnapConfig,
        defaults: &'a ToolDefaults,
    ) -> ToolContext<'a> {
        ToolContext {
            phase,
            cursor: Some(cursor),
            raw_cursor: Some(cursor),
            over_ui: false,
            confirm: false,
            cancel: false,
            secondary,
            constraints,
            snap,
            defaults,
            cam_scale: 1.0,
        }
    }

    #[test]
    fn the_last_point_commits_once_with_every_point() {
        let (c, s, d) = (
            GestureConstraints::default(),
            SnapConfig::default(),
            ToolDefaults::default(),
        );
        let mut tool = ScriptTool::default();
        tool.select(Some((3, 2)));
        let press = |p| ctx(GesturePhase::Pressed, GesturePhase::Idle, p, &c, &s, &d);
        assert!(tool.update(&press(Vec2::ZERO)).is_none());
        assert!(tool.drafting());
        assert_eq!(
            tool.snap_reference(),
            Some(DraftReference::Line(Vec2::ZERO))
        );
        let Some(ToolCommit::Script {
            tool: index,
            points,
        }) = tool.update(&press(Vec2::X))
        else {
            panic!("expected a script commit");
        };
        assert_eq!(index, 3);
        assert_eq!(points, vec![Vec2::ZERO, Vec2::X]);
        assert!(!tool.drafting());
    }

    #[test]
    fn a_right_click_cancels_and_an_unknown_tool_does_nothing() {
        let (c, s, d) = (
            GestureConstraints::default(),
            SnapConfig::default(),
            ToolDefaults::default(),
        );
        let mut tool = ScriptTool::default();
        let press = ctx(
            GesturePhase::Pressed,
            GesturePhase::Idle,
            Vec2::ONE,
            &c,
            &s,
            &d,
        );
        assert!(tool.update(&press).is_none());
        assert!(!tool.drafting(), "no active tool collects nothing");

        tool.select(Some((0, 2)));
        assert!(tool.update(&press).is_none());
        let right = ctx(
            GesturePhase::Idle,
            GesturePhase::Pressed,
            Vec2::ONE,
            &c,
            &s,
            &d,
        );
        assert!(tool.update(&right).is_none());
        assert!(!tool.drafting(), "right-click drops the draft");
    }
}
