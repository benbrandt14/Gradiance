//! Constrained line-chain tool: the sketch-mode counterpart of the polygon
//! tool.
//!
//! Where `polygon_tool` records whatever points you clicked, this one records
//! *intent*. Each new segment is examined and, when it is near-axis, gets a
//! real [`SketchConstraint::Horizontal`] / [`SketchConstraint::Vertical`]
//! attached — then the whole sketch is re-solved, so the geometry visibly
//! snaps square and **stays** square as later segments move it. That
//! inference is the difference between drawing a polygon and drawing a sketch.
//!
//! Segments share point identities at their joins, so chain continuity is
//! structural rather than a coincidence constraint the solver has to satisfy.

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;
use bevy::color::palettes::css;
use bevy::prelude::*;
use gradiance_sketch::doc::{SketchConstraint, SketchDoc, SketchId};
use gradiance_sketch::pick::{self, SketchTarget, SnapKind};
use gradiance_sketch::{lower, solve};

/// Clicking within this distance of the first point closes the loop.
const CLOSE_RADIUS: f32 = 0.08;

/// A segment within this many degrees of an axis is taken to be *meant* as
/// axis-aligned, and gets a constraint rather than just happening to look
/// straight. Matches the inference tolerance CAD sketchers expect.
const AXIS_SNAP_DEGREES: f32 = 5.0;

/// Snap radius in logical screen pixels, converted to world units with the
/// camera scale so snapping feels the same at every zoom level.
const SNAP_PIXELS: f32 = 10.0;

/// An in-progress constrained sketch.
#[derive(Resource, Default, Debug)]
pub struct SketchLineTool {
    /// The sketch being authored.
    doc: SketchDoc,
    /// Point identities in chain order; `chain[0]` is where the loop closes.
    chain: Vec<SketchId>,
    /// Remaining degrees of freedom from the last solve, for the readout.
    dof: Option<i32>,
    /// The snap candidate under the cursor as of the last frame, so the
    /// preview can show what a click would attach to.
    hover: Option<pick::PickHit>,
}

impl SketchLineTool {
    /// Remaining degrees of freedom, once something has been solved.
    ///
    /// `Some(0)` means fully constrained — the state CAD users drive toward.
    pub fn dof(&self) -> Option<i32> {
        self.dof
    }

    /// The sketch as it currently stands.
    pub fn doc(&self) -> &SketchDoc {
        &self.doc
    }

    /// Discard any in-progress draft.
    ///
    /// Leaving sketch mode must not leave a half-drawn chain waiting to
    /// reappear the next time the mode is entered.
    pub fn abandon(&mut self) {
        self.clear();
    }

    fn clear(&mut self) {
        self.doc = SketchDoc::new();
        self.chain.clear();
        self.dof = None;
        self.hover = None;
    }

    /// The snap candidate under the cursor, if any.
    pub fn hover(&self) -> Option<pick::PickHit> {
        self.hover
    }

    /// Resolve a click into a point to chain from, attaching whatever
    /// constraint the snap implies.
    ///
    /// This is where snapping becomes *parametric*: landing on an existing
    /// point reuses its identity (so the join is structural and the solver
    /// never has to satisfy a redundant coincidence), and landing along an
    /// existing line adds a real `PointOnLine` constraint, so the new vertex
    /// keeps sliding on that line as the sketch is re-solved rather than
    /// merely starting out near it.
    fn point_for_click(&mut self, cursor: Vec2, tol: f32) -> SketchId {
        match pick::pick(&self.doc, cursor, tol) {
            Some(hit) => match hit.target {
                // Reuse the identity outright — a shared point beats a
                // coincidence constraint between two coincident points.
                SketchTarget::Point(id) => id,
                SketchTarget::Entity(entity) => {
                    let id = self.doc.add_point(hit.at);
                    match hit.kind {
                        SnapKind::Midpoint => self.doc.constrain(SketchConstraint::Midpoint {
                            point: id,
                            line: entity,
                        }),
                        // A centre snap resolved to an entity means the centre
                        // point itself was not the nearest feature; treat it as
                        // a plain placement rather than inventing a relation.
                        SnapKind::Center | SnapKind::Point => {}
                        SnapKind::OnEntity => self.doc.constrain(SketchConstraint::PointOnLine {
                            point: id,
                            line: entity,
                        }),
                    }
                    id
                }
            },
            None => self.doc.add_point(cursor),
        }
    }

    /// World position of a chain point after the last solve.
    fn at(&self, id: SketchId) -> Option<Vec2> {
        self.doc.point(id).map(|p| p.at)
    }

    /// Infer an axis constraint for the segment `a`->`b`, if it reads as one.
    ///
    /// Returns the constraint to attach to `line`, or `None` when the segment
    /// is clearly diagonal and the author meant it that way.
    fn infer_axis(&self, a: SketchId, b: SketchId, line: SketchId) -> Option<SketchConstraint> {
        let (pa, pb) = (self.at(a)?, self.at(b)?);
        let d = pb - pa;
        if d.length_squared() < f32::EPSILON {
            return None;
        }
        let tol = AXIS_SNAP_DEGREES.to_radians().tan();
        if d.y.abs() <= tol * d.x.abs() {
            Some(SketchConstraint::Horizontal(line))
        } else if d.x.abs() <= tol * d.y.abs() {
            Some(SketchConstraint::Vertical(line))
        } else {
            None
        }
    }

    /// Re-solve, keeping the last good geometry if the solver refuses.
    ///
    /// A rejected inference must never strand the draft: the solver leaves the
    /// document untouched on failure, so the worst case is that the segment
    /// stays where it was drawn.
    fn resolve(&mut self) {
        if let Ok(outcome) = solve::solve(&mut self.doc, None) {
            self.dof = Some(outcome.dof);
        }
    }

    /// Close the loop and hand back a body, clearing the draft either way.
    fn finish(&mut self) -> Option<ToolCommit> {
        let closed = self.chain.len() >= 3;
        if closed && let (Some(&last), Some(&first)) = (self.chain.last(), self.chain.first()) {
            let line = self.doc.add_line(last, first);
            if let Some(c) = self.infer_axis(last, first, line) {
                self.doc.constrain(c);
            }
            self.resolve();
        }

        let doc = std::mem::take(&mut self.doc);
        let result = closed
            .then(|| lower::to_shape_with_origin(&doc))
            .flatten_ok();
        self.clear();

        let (shape, origin) = result?;
        shape.validate().ok()?;
        let mut record = new_body_record(shape, origin, 0.0);
        // The sketch rides along with the body: this is what makes the body
        // re-openable for constraint editing, and it is saved and undone as
        // one unit with the geometry it produced.
        record.sketch = Some(doc);
        Some(ToolCommit::SpawnBody(Box::new(record)))
    }
}

/// `Option<Result<T, E>>` -> `Option<T>`, discarding the error.
trait FlattenOk<T> {
    fn flatten_ok(self) -> Option<T>;
}

impl<T, E> FlattenOk<T> for Option<Result<T, E>> {
    fn flatten_ok(self) -> Option<T> {
        self.and_then(std::result::Result::ok)
    }
}

impl DraftTool for SketchLineTool {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if ctx.cancel {
            self.clear();
            return None;
        }
        if ctx.confirm && self.chain.len() >= 3 {
            return self.finish();
        }
        // Hover tracking runs every frame, not just on press, so the preview
        // can show what a click would snap to before it happens.
        self.hover = ctx
            .cursor
            .and_then(|c| pick::pick(&self.doc, c, SNAP_PIXELS * ctx.cam_scale));

        if ctx.phase != GesturePhase::Pressed {
            return None;
        }
        let p = ctx.cursor?;

        let closes = self.chain.len() >= 3
            && self
                .chain
                .first()
                .and_then(|&f| self.at(f))
                .is_some_and(|f| f.distance(p) <= CLOSE_RADIUS);
        if closes {
            return self.finish();
        }

        let id = self.point_for_click(p, SNAP_PIXELS * ctx.cam_scale);
        // Clicking the point we are already chaining from would make a
        // zero-length segment; ignore it rather than feeding the solver a
        // degenerate line.
        if self.chain.last() == Some(&id) {
            return None;
        }
        if let Some(&prev) = self.chain.last() {
            let line = self.doc.add_line(prev, id);
            if let Some(c) = self.infer_axis(prev, id, line) {
                self.doc.constrain(c);
            }
            // Pin the first point so inference resolves by moving the *new*
            // geometry rather than sliding the whole chain around.
            if let Some(&first) = self.chain.first()
                && let Some(p0) = self.doc.point_mut(first)
            {
                p0.fixed = true;
            }
            self.chain.push(id);
            self.resolve();
        } else {
            self.chain.push(id);
        }
        None
    }

    fn drafting(&self) -> bool {
        !self.chain.is_empty()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        if self.chain.is_empty() {
            return;
        }
        let pts: Vec<Vec2> = self.chain.iter().filter_map(|&id| self.at(id)).collect();
        if pts.len() >= 2 {
            out.polyline(pts.clone(), css::AQUAMARINE);
        }
        if let (Some(&last), Some(p)) = (self.chain.last(), ctx.cursor)
            && let Some(a) = self.at(last)
        {
            out.line(a, p, css::AQUAMARINE.with_alpha(0.5));
        }
        // The snap marker: what a click would attach to.
        if let Some(hit) = self.hover {
            let color = match hit.kind {
                SnapKind::Point => css::ORANGE,
                SnapKind::Midpoint => css::YELLOW,
                SnapKind::Center => css::MAGENTA,
                SnapKind::OnEntity => css::AQUA,
            };
            out.circle(hit.at, ctx.cam_scale * 5.0, color);
        }

        // The closing hint, so it is obvious where the loop completes.
        if pts.len() >= 3
            && let (Some(first), Some(p)) = (pts.first().copied(), ctx.cursor)
            && first.distance(p) <= CLOSE_RADIUS
        {
            out.line(p, first, css::LIME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gesture::GestureConstraints;
    use gradiance_domain::settings::SnapConfig;

    fn ctx<'a>(
        phase: GesturePhase,
        cursor: Option<Vec2>,
        constraints: &'a GestureConstraints,
        snap: &'a SnapConfig,
    ) -> ToolContext<'a> {
        ToolContext {
            phase,
            cursor,
            raw_cursor: cursor,
            over_ui: false,
            confirm: false,
            cancel: false,
            constraints,
            snap,
            // World units per logical pixel. At PIXELS_PER_METER = 100 a 1:1
            // view is 0.01, which puts the snap radius at a realistic 0.1 m.
            // Leaving this at 1.0 would give a ten-metre snap radius and make
            // every click land on the previous point.
            cam_scale: 0.01,
        }
    }

    /// Click a sequence of points, returning any commit the last click made.
    fn click_all(tool: &mut SketchLineTool, pts: &[Vec2]) -> Option<ToolCommit> {
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mut last = None;
        for p in pts {
            last = tool.update(&ctx(GesturePhase::Pressed, Some(*p), &gc, &sc));
        }
        last
    }

    #[test]
    fn a_near_horizontal_segment_is_solved_exactly_horizontal() {
        let mut t = SketchLineTool::default();
        // Second point is 2 degrees off horizontal — within the inference
        // tolerance, so it should be *made* horizontal, not left as drawn.
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.035)]);

        let pts: Vec<Vec2> = t.chain.iter().filter_map(|&id| t.at(id)).collect();
        assert_eq!(pts.len(), 2);
        assert!(
            (pts[1].y - pts[0].y).abs() < 1e-4,
            "expected the solver to flatten the segment, got {pts:?}"
        );
    }

    #[test]
    fn a_clearly_diagonal_segment_is_left_alone() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)]);

        let pts: Vec<Vec2> = t.chain.iter().filter_map(|&id| t.at(id)).collect();
        assert!(
            (pts[1] - Vec2::new(1.0, 1.0)).length() < 1e-4,
            "a 45-degree segment must not be snapped to an axis, got {pts:?}"
        );
        assert!(
            t.doc.constraints.is_empty(),
            "no constraint should have been inferred"
        );
    }

    #[test]
    fn closing_the_loop_commits_a_body_carrying_its_sketch() {
        let mut t = SketchLineTool::default();
        let square = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.0, 2.0),
        ];
        assert!(click_all(&mut t, &square).is_none(), "still drafting");

        // Click back near the start to close.
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let commit = t.update(&ctx(
            GesturePhase::Pressed,
            Some(Vec2::new(0.01, 0.01)),
            &gc,
            &sc,
        ));

        let Some(ToolCommit::SpawnBody(record)) = commit else {
            panic!("closing the loop should spawn a body, got {commit:?}");
        };
        let doc = record.sketch.expect("body must retain its sketch");
        assert_eq!(doc.points.len(), 4);
        assert_eq!(doc.entities.len(), 4, "four sides");
        assert!(
            !doc.constraints.is_empty(),
            "axis inference should have recorded constraints"
        );
        // The draft is reset so the next gesture starts clean.
        assert!(!t.drafting());
    }

    #[test]
    fn clicking_an_existing_point_reuses_its_identity() {
        let mut t = SketchLineTool::default();
        // Only two points: short of the three that would make a click near the
        // start close the loop instead, so this exercises identity reuse
        // rather than the closing path.
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0)]);
        let first = t.chain[0];
        let before = t.doc().points.len();

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        assert!(
            t.update(&ctx(
                GesturePhase::Pressed,
                Some(Vec2::new(0.002, 0.001)),
                &gc,
                &sc,
            ))
            .is_none(),
            "two segments cannot enclose an area yet"
        );

        assert_eq!(
            t.doc().points.len(),
            before,
            "snapping to a point must reuse it, not mint a coincident duplicate"
        );
        assert_eq!(
            t.chain.last(),
            Some(&first),
            "the new segment should terminate on the original point"
        );
    }

    #[test]
    fn clicking_along_an_existing_line_records_point_on_line() {
        let mut t = SketchLineTool::default();
        // A horizontal segment from (0,0) to (4,0).
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0)]);
        let constraints_before = t.doc().constraints.len();

        // Click just off the middle-ish of that line, but not its midpoint.
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        t.update(&ctx(
            GesturePhase::Pressed,
            Some(Vec2::new(3.0, 0.004)),
            &gc,
            &sc,
        ));

        let added: Vec<_> = t.doc().constraints[constraints_before..].to_vec();
        assert!(
            added
                .iter()
                .any(|c| matches!(c, SketchConstraint::PointOnLine { .. })),
            "landing on a line should pin the new point to it, got {added:?}"
        );
    }

    #[test]
    fn clicking_a_line_midpoint_records_a_midpoint_constraint() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0)]);
        let before = t.doc().constraints.len();

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        t.update(&ctx(
            GesturePhase::Pressed,
            Some(Vec2::new(2.001, 0.001)),
            &gc,
            &sc,
        ));

        let added: Vec<_> = t.doc().constraints[before..].to_vec();
        assert!(
            added
                .iter()
                .any(|c| matches!(c, SketchConstraint::Midpoint { .. })),
            "the midpoint snap should say midpoint, not just point-on-line: {added:?}"
        );
    }

    #[test]
    fn a_click_far_from_anything_snaps_to_nothing() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0)]);
        let before = t.doc().constraints.len();

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        t.update(&ctx(
            GesturePhase::Pressed,
            Some(Vec2::new(3.0, 9.0)),
            &gc,
            &sc,
        ));
        assert_eq!(
            t.doc().constraints.len(),
            before,
            "an unsnapped click must not invent a relationship"
        );
    }

    #[test]
    fn hover_reports_a_snap_candidate_without_clicking() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0)]);

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        assert!(
            t.update(&ctx(
                GesturePhase::Idle,
                Some(Vec2::new(2.0, 0.002)),
                &gc,
                &sc
            ))
            .is_none(),
            "hovering must not commit anything"
        );
        let hit = t.hover().expect("hovering near the midpoint should snap");
        assert_eq!(hit.kind, gradiance_sketch::pick::SnapKind::Midpoint);
    }

    #[test]
    fn escape_abandons_the_draft() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::ZERO, Vec2::new(1.0, 0.0)]);
        assert!(t.drafting());

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mut c = ctx(GesturePhase::Idle, None, &gc, &sc);
        c.cancel = true;
        assert!(t.update(&c).is_none());
        assert!(!t.drafting());
        assert!(t.doc().points.is_empty(), "document must be cleared too");
    }

    #[test]
    fn an_unclosed_chain_commits_nothing() {
        let mut t = SketchLineTool::default();
        click_all(&mut t, &[Vec2::ZERO, Vec2::new(1.0, 0.0)]);

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mut c = ctx(GesturePhase::Idle, None, &gc, &sc);
        c.confirm = true;
        assert!(
            t.update(&c).is_none(),
            "two points cannot enclose an area, so there is nothing to commit"
        );
    }

    #[test]
    fn degrees_of_freedom_are_reported_once_solving_starts() {
        let mut t = SketchLineTool::default();
        assert_eq!(t.dof(), None, "nothing solved yet");
        click_all(&mut t, &[Vec2::ZERO, Vec2::new(1.0, 0.0)]);
        assert!(t.dof().is_some(), "a solve should have reported its dof");
    }
}
