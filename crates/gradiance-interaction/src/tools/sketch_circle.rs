//! Sketch-mode circle tool: drag from centre to rim.
//!
//! The radius is a solver parameter rather than a baked number, so a later
//! `Diameter` constraint can drive it — which is the whole reason a sketched
//! circle differs from the one `circle_tool` produces.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_sketch::doc::SketchDoc;
use gradiance_sketch::lower;

/// Below this radius the drag is treated as a stray click, not a circle.
const MIN_RADIUS: f32 = 0.01;

/// An in-progress sketched circle.
#[derive(Resource, Default, Debug)]
pub struct SketchCircleTool {
    /// Centre in world space, set on press.
    center: Option<Vec2>,
    /// Live radius while dragging.
    radius: f32,
}

impl DraftTool for SketchCircleTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if ctx.cancel {
            self.center = None;
            return None;
        }
        match ctx.phase {
            GesturePhase::Pressed => {
                self.center = ctx.cursor;
                self.radius = 0.0;
                None
            }
            GesturePhase::Held => {
                if let (Some(c), Some(p)) = (self.center, ctx.cursor) {
                    self.radius = c.distance(p);
                }
                None
            }
            GesturePhase::Released => {
                let c = self.center.take()?;
                let r = self.radius;
                if r < MIN_RADIUS {
                    return None;
                }
                let mut doc = SketchDoc::new();
                let center = doc.add_point(Vec2::ZERO);
                // The centre is the body origin, so pin it: the circle's one
                // remaining freedom is its radius.
                if let Some(p) = doc.point_mut(center) {
                    p.fixed = true;
                }
                doc.add_circle(center, r);

                let shape = lower::to_shape(&doc).ok()?;
                shape.validate().ok()?;
                let mut record = new_body_record(shape, c, 0.0);
                record.sketch = Some(doc);
                Some(ToolCommit::SpawnBody(Box::new(record)))
            }
            GesturePhase::Idle => None,
        }
    }

    fn drafting(&self) -> bool {
        self.center.is_some()
    }

    fn preview(&self, _ctx: &ToolContext, out: &mut ToolPreview) {
        if let Some(c) = self.center
            && self.radius >= MIN_RADIUS
        {
            out.circle(c, self.radius, css::AQUAMARINE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::GestureConstraints;
    use gradiance_domain::settings::SnapConfig;

    fn drag(from: Vec2, to: Vec2) -> Option<ToolCommit> {
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mk = |phase, cursor| ToolContext {
            phase,
            cursor: Some(cursor),
            raw_cursor: Some(cursor),
            over_ui: false,
            confirm: false,
            cancel: false,
            constraints: &gc,
            snap: &sc,
            cam_scale: 1.0,
        };
        let mut t = SketchCircleTool::default();
        t.update(&mk(GesturePhase::Pressed, from));
        t.update(&mk(GesturePhase::Held, to));
        t.update(&mk(GesturePhase::Released, to))
    }

    #[test]
    fn a_drag_spawns_a_circle_body_retaining_its_sketch() {
        let commit = drag(Vec2::new(5.0, 5.0), Vec2::new(7.0, 5.0));
        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("expected a body, got {commit:?}");
        };
        // Placed at the centre of the drag, not at the rim.
        assert!((record.pose.pos - Vec2::new(5.0, 5.0)).length() < 1e-4);

        let doc = record.sketch.expect("sketched circle keeps its document");
        assert_eq!(doc.entities.len(), 1);
        assert_eq!(doc.points.len(), 1, "just the centre");
        assert!(
            doc.points[0].fixed,
            "the centre is the body origin and must be pinned"
        );
    }

    #[test]
    fn a_stray_click_commits_nothing() {
        assert!(
            drag(Vec2::ZERO, Vec2::new(0.001, 0.0)).is_none(),
            "a sub-threshold drag is a click, not a circle"
        );
    }
}
