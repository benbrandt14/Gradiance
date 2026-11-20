//! The sketch tool (brief N-6): draw freehand; release makes one body.
//!
//! The drag records the stroke from the unsnapped pointer (a freehand line
//! follows the hand, not the grid). Memory is bounded: past
//! [`MAX_STROKE_POINTS`] the draft keeps every other point and records half
//! as often. On release the tool commits one [`ToolCommit::Sketch`]: the
//! stroke (thinned to within a pixel) and the width from [`ToolDefaults`](gradiance_domain::settings::ToolDefaults).
//! The commit invokes the `body.sketch` operation, which builds the shape
//! (`geometry::sketch`) and spawns it as **one** undo step; during the drag
//! only the gizmo preview exists.

use crate::selection::Selection;
use crate::tools::context::{
    GesturePhase, ManipContext, ManipOutput, ManipTool, ToolCommit, ToolPreview, ToolWorld,
};
use bevy::prelude::*;
use gradiance_domain::theme::Role;
use gradiance_geometry::sketch::{MAX_STROKE_POINTS, rdp, sketch_shape};

/// The stroke records a point each time the pointer has moved this far
/// (screen pixels), before any thinning.
const SAMPLE_PX: f32 = 3.0;
/// The committed stroke is thinned (RDP) to within this many screen
/// pixels: it keeps the drawn shape and drops the points a straight run
/// does not need, so the operation's parameters stay short.
const THIN_PX: f32 = 1.0;

/// In-progress sketch gesture: the stroke so far (empty when idle).
#[derive(Resource, Debug)]
pub struct SketchDraft {
    /// The recorded stroke, world space, in drawing order.
    pub path: Vec<Vec2>,
    /// The sampling distance multiplier: doubles each time the stroke is
    /// halved to stay within [`MAX_STROKE_POINTS`].
    spacing: f32,
}

impl Default for SketchDraft {
    fn default() -> Self {
        Self {
            path: Vec::new(),
            spacing: 1.0,
        }
    }
}

impl SketchDraft {
    /// Starts a stroke at `p`.
    fn begin(&mut self, p: Vec2) {
        self.path.clear();
        self.spacing = 1.0;
        self.path.push(p);
    }

    /// Adds `p` to the stroke when it is at least `step` (times the
    /// current spacing) from the last recorded point. At
    /// [`MAX_STROKE_POINTS`] the stroke drops every other point (keeping
    /// both ends) and the spacing doubles, so the stroke never outgrows the
    /// cap.
    fn record(&mut self, p: Vec2, step: f32) {
        if self
            .path
            .last()
            .is_some_and(|last| last.distance(p) < step * self.spacing)
        {
            return;
        }
        self.path.push(p);
        if self.path.len() >= MAX_STROKE_POINTS {
            let last = self.path.len() - 1;
            let mut i = 0;
            self.path.retain(|_| {
                let keep = i % 2 == 0 || i == last;
                i += 1;
                keep
            });
            self.spacing *= 2.0;
        }
    }

    /// The stroke with the pointer's current point as its end.
    fn stroke_to(&self, end: Option<Vec2>) -> Vec<Vec2> {
        let mut path = self.path.clone();
        if let Some(end) = end
            && path.last() != Some(&end)
        {
            path.push(end);
        }
        path
    }
}

impl ManipTool for SketchDraft {
    fn update(
        &mut self,
        ctx: &ManipContext,
        _world: &ToolWorld,
        _selection: &Selection,
    ) -> ManipOutput {
        let pointer = ctx.raw_cursor.or(ctx.cursor);
        match ctx.left {
            GesturePhase::Pressed => match pointer {
                Some(p) => self.begin(p),
                None => self.path.clear(),
            },
            GesturePhase::Held => {
                if !self.path.is_empty()
                    && let Some(p) = pointer
                {
                    self.record(p, SAMPLE_PX * ctx.cam_scale);
                }
            }
            GesturePhase::Released if !self.path.is_empty() => {
                let stroke = rdp(&self.stroke_to(pointer), THIN_PX * ctx.cam_scale);
                self.path.clear();
                let width = ctx.defaults.sketch_width.0;
                if sketch_shape(&stroke, width / 2.0).is_some() {
                    return ManipOutput::commit(ToolCommit::Sketch {
                        path: stroke,
                        width,
                    });
                }
            }
            GesturePhase::Released | GesturePhase::Idle => {}
        }
        ManipOutput::default()
    }

    fn drafting(&self) -> bool {
        !self.path.is_empty()
    }

    fn preview(&self, ctx: &ManipContext, out: &mut ToolPreview) {
        let path = self.stroke_to(ctx.raw_cursor.or(ctx.cursor));
        if path.len() < 2 {
            return;
        }
        // The hand's stroke, faint; over it, what release would make: the
        // simplified centre line with a ring of the stroke's width at each
        // joint, or the closed outline a loop fills.
        out.polyline(path.clone(), Role::Preview.alpha(0.4));
        let Some(sketch) = sketch_shape(&path, ctx.defaults.sketch_width.0 / 2.0) else {
            return;
        };
        let mut line = sketch.line;
        if sketch.closed {
            if let Some(first) = line.first().copied() {
                line.push(first);
            }
            out.polyline(line, Role::Preview);
        } else {
            let radius = sketch.radius.max(ctx.cam_scale);
            for p in &line {
                out.circle(*p, radius, Role::Preview.alpha(0.6));
            }
            out.polyline(line, Role::Preview);
        }
    }
}

/// The `body.sketch` invocation for a sketch commit: the operation builds
/// the shape, so a drawn sketch and a scripted one are the same record and
/// Adjust Last Operation can change its width afterwards.
pub fn sketch_invocation(path: &[Vec2], width: f32) -> gradiance_op::InvokeOp {
    let flat: Vec<f32> = path.iter().flat_map(|p| [p.x, p.y]).collect();
    gradiance_op::InvokeOp::with(
        "body.sketch",
        gradiance_op::json!({ "path": flat, "width": width }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stroke_records_points_a_step_apart_and_ends_at_the_pointer() {
        let mut draft = SketchDraft::default();
        draft.begin(Vec2::ZERO);
        draft.record(Vec2::new(0.05, 0.0), 0.1);
        draft.record(Vec2::new(0.2, 0.0), 0.1);
        assert_eq!(draft.path, vec![Vec2::ZERO, Vec2::new(0.2, 0.0)]);
        assert_eq!(draft.stroke_to(Some(Vec2::new(0.25, 0.0))).len(), 3);
        assert_eq!(draft.stroke_to(Some(Vec2::new(0.2, 0.0))).len(), 2);
    }

    #[test]
    fn a_long_drag_never_outgrows_the_point_cap() {
        let mut draft = SketchDraft::default();
        draft.begin(Vec2::ZERO);
        for i in 1..20_000 {
            draft.record(Vec2::new(i as f32 * 0.01, 0.0), 0.01);
            assert!(draft.path.len() < MAX_STROKE_POINTS);
        }
        // The whole stroke is still covered, start to end.
        assert_eq!(draft.path.first(), Some(&Vec2::ZERO));
        assert!(draft.path.last().is_some_and(|p| p.x > 199.0));
        assert!(draft.path.len() > MAX_STROKE_POINTS / 4);
    }

    #[test]
    fn the_invocation_flattens_the_path() {
        let invoke = sketch_invocation(&[Vec2::ZERO, Vec2::new(1.0, 2.0)], 0.1);
        assert_eq!(invoke.id, "body.sketch");
        assert_eq!(
            invoke.params["path"],
            gradiance_op::json!([0.0, 0.0, 1.0, 2.0])
        );
        assert!((invoke.params["width"].as_f64().unwrap_or(0.0) - 0.1).abs() < 1e-6);
    }
}
