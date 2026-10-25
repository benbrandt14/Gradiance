//! Box creation tool: drag out a rectangle, or (with
//! `ToolDefaults::box_three_point`) click three points.
//!
//! The two-corner drag is the default: press at one corner, release at the
//! opposite one, and an axis-aligned box spans them. The **three-point**
//! constructor draws a rotated box: the first two clicks set one edge — its
//! length *and* rotation — and the third sets the height, to whichever side
//! of that edge it falls ([`three_point_box`]). Clicks are presses, like the
//! polygon tool's vertices; Escape cancels at any point, and a click that
//! cannot advance the draft (the second on top of the first, the third on
//! the edge's line) is ignored so a stray double-click loses nothing. The
//! a → b edge honours the axis lock and, with `Ctrl`, the rotation step.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::construct::{RotatedBox, three_point_box};

/// Minimum accepted box side, world metres (~1 px on screen). The SI flip
/// left this at its pixel-era `1.0`, which demanded a 1 m (100 px) minimum
/// side and silently swallowed small draws.
const MIN_SIDE: f32 = 0.01;

/// The box tool's in-progress draft.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BoxDraft {
    /// Nothing in progress.
    #[default]
    Idle,
    /// Two-corner mode: the pressed anchor corner.
    Corner(Vec2),
    /// Three-point mode: the first edge point is placed.
    EdgeStart(Vec2),
    /// Three-point mode: the edge `a → b` is placed; the cursor sets height.
    Height(Vec2, Vec2),
}

/// In-progress box draft.
#[derive(Resource, Default, Debug)]
pub struct BoxTool(pub BoxDraft);

/// The edge's far end for a cursor at `p`: the delta from `a` under the axis
/// lock, then (with the quantize modifier) turned to the rotation step.
fn edge_end(a: Vec2, p: Vec2, ctx: &ToolContext) -> Vec2 {
    let delta = ctx.constraints.constrain(p - a);
    if !ctx.constraints.quantize_rotation || delta == Vec2::ZERO {
        return a + delta;
    }
    let angle = ctx.constraints.apply_rotation(delta.to_angle(), ctx.snap);
    a + Vec2::from_angle(angle) * delta.length()
}

/// The finished three-point box for a third point at `c`, if large enough.
fn placed_box(a: Vec2, b: Vec2, c: Vec2) -> Option<RotatedBox> {
    three_point_box(a, b, c).filter(|bx| bx.width >= MIN_SIDE && bx.height >= MIN_SIDE)
}

impl BoxTool {
    /// Two-corner drag: press sets the anchor, release commits.
    fn update_corners(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        match ctx.phase {
            GesturePhase::Pressed => {
                self.0 = ctx.cursor.map_or(BoxDraft::Idle, BoxDraft::Corner);
                None
            }
            GesturePhase::Released => {
                let BoxDraft::Corner(anchor) = std::mem::take(&mut self.0) else {
                    return None;
                };
                let p = ctx.cursor?;
                let size = (p - anchor).abs();
                (size.x >= MIN_SIDE && size.y >= MIN_SIDE).then(|| {
                    ToolCommit::SpawnBody(Box::new(new_body_record(
                        ShapeDef::Box {
                            width: size.x,
                            height: size.y,
                        },
                        (anchor + p) / 2.0,
                        0.0,
                    )))
                })
            }
            GesturePhase::Idle | GesturePhase::Held => None,
        }
    }

    /// Three clicks: edge start, edge end, height. Commits on the third.
    fn update_three_point(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if ctx.phase != GesturePhase::Pressed {
            return None;
        }
        let p = ctx.cursor?;
        match self.0 {
            BoxDraft::Idle | BoxDraft::Corner(_) => {
                self.0 = BoxDraft::EdgeStart(p);
                None
            }
            BoxDraft::EdgeStart(a) => {
                let b = edge_end(a, p, ctx);
                if a.distance(b) >= MIN_SIDE {
                    self.0 = BoxDraft::Height(a, b);
                }
                None
            }
            BoxDraft::Height(a, b) => {
                let bx = placed_box(a, b, p)?;
                self.0 = BoxDraft::Idle;
                Some(ToolCommit::SpawnBody(Box::new(new_body_record(
                    ShapeDef::Box {
                        width: bx.width,
                        height: bx.height,
                    },
                    bx.center,
                    bx.rotation,
                ))))
            }
        }
    }
}

impl DraftTool for BoxTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if ctx.cancel {
            self.0 = BoxDraft::Idle;
            return None;
        }
        if ctx.defaults.box_three_point {
            self.update_three_point(ctx)
        } else {
            // A three-point draft left over from a mode switch is dropped.
            if matches!(self.0, BoxDraft::EdgeStart(_) | BoxDraft::Height(..)) {
                self.0 = BoxDraft::Idle;
            }
            self.update_corners(ctx)
        }
    }

    fn drafting(&self) -> bool {
        self.0 != BoxDraft::Idle
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        let Some(p) = ctx.cursor else {
            return;
        };
        match self.0 {
            BoxDraft::Idle => {}
            BoxDraft::Corner(anchor) => {
                out.rect((anchor + p) / 2.0, (p - anchor).abs(), css::AQUAMARINE);
            }
            BoxDraft::EdgeStart(a) => {
                out.line(a, edge_end(a, p, ctx), css::AQUAMARINE);
            }
            BoxDraft::Height(a, b) => {
                if let Some(bx) = three_point_box(a, b, p) {
                    let [c0, c1, c2, c3] = bx.corners();
                    out.polyline(vec![c0, c1, c2, c3, c0], css::AQUAMARINE.with_alpha(0.6));
                }
                out.line(a, b, css::AQUAMARINE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::GestureConstraints;
    use crate::tools::context::ToolCommit;
    use gradiance_domain::settings::{SnapConfig, ToolDefaults};

    /// The two-corner (default) tool defaults, shared by the drag tests.
    const CORNERS: &ToolDefaults = &ToolDefaults {
        slider_limits: true,
        box_three_point: false,
    };

    /// The three-point constructor's tool defaults.
    const THREE_POINT: &ToolDefaults = &ToolDefaults {
        slider_limits: true,
        box_three_point: true,
    };

    /// Builds a two-corner-mode context at a phase/cursor with default
    /// constraints — the same shape a scripted tool would receive (no ECS
    /// involved).
    fn ctx<'a>(
        phase: GesturePhase,
        cursor: Vec2,
        constraints: &'a GestureConstraints,
        snap: &'a SnapConfig,
    ) -> ToolContext<'a> {
        ctx_with(phase, cursor, constraints, snap, CORNERS)
    }

    /// [`ctx`] with explicit tool defaults (the constructor mode).
    fn ctx_with<'a>(
        phase: GesturePhase,
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
            constraints,
            snap,
            defaults,
            cam_scale: 1.0,
        }
    }

    /// A three-point-mode click (press) at `p`.
    fn click(
        tool: &mut BoxTool,
        p: Vec2,
        constraints: &GestureConstraints,
        snap: &SnapConfig,
    ) -> Option<ToolCommit> {
        let commit = tool.update(&ctx_with(
            GesturePhase::Pressed,
            p,
            constraints,
            snap,
            THREE_POINT,
        ));
        // The release that ends each click must not commit or advance.
        let released = tool.update(&ctx_with(
            GesturePhase::Released,
            p,
            constraints,
            snap,
            THREE_POINT,
        ));
        assert!(
            released.is_none(),
            "a release never commits in 3-point mode"
        );
        commit
    }

    #[test]
    fn three_clicks_commit_one_rotated_box() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        let a = Vec2::new(1.0, 1.0);
        let b = a + Vec2::new(3.0, 3.0);
        // Two units to the right of a → b (direction (1, -1)/√2).
        let c = b + Vec2::new(1.0, -1.0).normalize() * 2.0;

        assert!(click(&mut tool, a, &constraints, &snap).is_none());
        assert_eq!(tool.0, BoxDraft::EdgeStart(a));
        assert!(click(&mut tool, b, &constraints, &snap).is_none());
        assert_eq!(tool.0, BoxDraft::Height(a, b));
        let commit = click(&mut tool, c, &constraints, &snap);

        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("third click commits a SpawnBody, got {commit:?}");
        };
        let ShapeDef::Box { width, height } = record.shape else {
            panic!("a box shape, got {:?}", record.shape);
        };
        assert!((width - 18f32.sqrt()).abs() < 1e-5, "width = |b - a|");
        assert!((height - 2.0).abs() < 1e-5, "height = distance of c");
        assert!(
            (record.pose.rot - std::f32::consts::FRAC_PI_4).abs() < 1e-5,
            "rotation = angle of b - a"
        );
        let expected = (a + b) / 2.0 + Vec2::new(1.0, -1.0).normalize();
        assert!((record.pose.pos - expected).length() < 1e-5);
        assert!(!tool.drafting(), "draft cleared on commit");
    }

    #[test]
    fn escape_mid_draft_commits_nothing() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        click(&mut tool, Vec2::ZERO, &constraints, &snap);
        click(&mut tool, Vec2::new(4.0, 0.0), &constraints, &snap);
        let mut cancel = ctx_with(
            GesturePhase::Idle,
            Vec2::new(2.0, 2.0),
            &constraints,
            &snap,
            THREE_POINT,
        );
        cancel.cancel = true;
        assert!(tool.update(&cancel).is_none());
        assert!(!tool.drafting(), "Escape clears the draft");
        // The next click starts a fresh edge rather than finishing a box.
        assert!(click(&mut tool, Vec2::new(2.0, 2.0), &constraints, &snap).is_none());
        assert_eq!(tool.0, BoxDraft::EdgeStart(Vec2::new(2.0, 2.0)));
    }

    #[test]
    fn clicks_that_cannot_advance_are_ignored() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        click(&mut tool, Vec2::ZERO, &constraints, &snap);
        click(&mut tool, Vec2::ZERO, &constraints, &snap);
        assert_eq!(tool.0, BoxDraft::EdgeStart(Vec2::ZERO), "zero edge ignored");
        click(&mut tool, Vec2::X, &constraints, &snap);
        assert!(
            click(&mut tool, Vec2::new(5.0, 0.0), &constraints, &snap).is_none(),
            "c on the edge's line"
        );
        assert_eq!(tool.0, BoxDraft::Height(Vec2::ZERO, Vec2::X));
    }

    #[test]
    fn the_edge_honours_the_axis_lock() {
        let constraints = GestureConstraints {
            axis: crate::gesture::AxisConstraint::Horizontal,
            ..GestureConstraints::default()
        };
        let snap = SnapConfig::default();
        let mut tool = BoxTool::default();
        click(&mut tool, Vec2::ZERO, &constraints, &snap);
        click(&mut tool, Vec2::new(3.0, 0.7), &constraints, &snap);
        assert_eq!(tool.0, BoxDraft::Height(Vec2::ZERO, Vec2::new(3.0, 0.0)));
        // The height click is not locked: it may lie anywhere off the edge.
        let commit = click(&mut tool, Vec2::new(9.0, 2.0), &constraints, &snap);
        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("third click commits, got {commit:?}");
        };
        assert!(record.pose.rot.abs() < 1e-6);
        assert!((record.pose.pos - Vec2::new(1.5, 1.0)).length() < 1e-5);
    }

    #[test]
    fn two_corner_mode_ignores_a_leftover_three_point_draft() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        click(&mut tool, Vec2::ZERO, &constraints, &snap);
        // Mode switched back to two-corner: a plain idle frame drops it.
        tool.update(&ctx(GesturePhase::Idle, Vec2::ONE, &constraints, &snap));
        assert!(!tool.drafting());
    }

    #[test]
    fn press_then_release_commits_one_centered_box() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();

        assert!(!tool.drafting(), "starts idle");
        assert!(
            tool.update(&ctx(GesturePhase::Pressed, Vec2::ZERO, &constraints, &snap))
                .is_none(),
            "press does not commit"
        );
        assert!(tool.drafting(), "drafting after press");

        let commit = tool.update(&ctx(
            GesturePhase::Released,
            Vec2::new(60.0, 40.0),
            &constraints,
            &snap,
        ));
        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("release commits a SpawnBody, got {commit:?}");
        };
        assert_eq!(
            record.shape,
            ShapeDef::Box {
                width: 60.0,
                height: 40.0
            }
        );
        assert!((record.pose.pos - Vec2::new(30.0, 20.0)).length() < 1e-6);
        assert!(!tool.drafting(), "draft cleared on commit");
    }

    #[test]
    fn a_zero_area_drag_commits_nothing() {
        let (constraints, snap) = (GestureConstraints::default(), SnapConfig::default());
        let mut tool = BoxTool::default();
        tool.update(&ctx(GesturePhase::Pressed, Vec2::ZERO, &constraints, &snap));
        let commit = tool.update(&ctx(
            GesturePhase::Released,
            Vec2::ZERO,
            &constraints,
            &snap,
        ));
        assert!(commit.is_none(), "sub-minimum box is rejected");
        assert!(!tool.drafting());
    }
}
