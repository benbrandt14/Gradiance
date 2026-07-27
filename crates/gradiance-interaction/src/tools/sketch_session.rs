//! The sketch-mode editing session: one document, one selection, every tool.
//!
//! Sketch mode is modal over a *single* document, which is the whole point.
//! When each tool carried its own [`SketchDoc`] a circle and a line could never
//! be named by the same constraint, so most of the constraint vocabulary was
//! unreachable no matter how good the solver was. Consolidating them here is
//! what makes "tangent to that arc" a thing the author can actually ask for.
//!
//! # Where this sits in the seams
//!
//! The session is a [`DraftTool`]: it accumulates *preview* state during a
//! gesture and emits exactly one [`ToolCommit`] when the sketch is committed,
//! so invariants 1 and 2 hold unchanged — nothing here writes an authored
//! component or touches the command stack. Editing the sketch document is not
//! world mutation; the document only becomes authored state when it rides into
//! a `BodyRecord` on commit.
//!
//! The UI drives it by *request*: [`SketchSession::request_commit`] sets a flag
//! that the next `update` consumes, rather than the panel manufacturing a
//! commit itself. One gesture, one command, one seam.

use bevy::color::palettes::css;
use bevy::prelude::*;

use gradiance_core::states::SketchTool;
use gradiance_sketch::doc::{SketchConstraint, SketchDoc, SketchEntity, SketchId};
use gradiance_sketch::edit::{self, ConstraintKind, SketchSelection};
use gradiance_sketch::ops;
use gradiance_sketch::pick::{self, PickHit, SketchTarget, SnapKind};
use gradiance_sketch::{lower, solve};

use crate::tools::context::{DraftTool, GesturePhase, ToolCommit, ToolContext, ToolPreview};
use crate::tools::new_body_record;

/// Clicking within this distance of the first point closes the loop.
const CLOSE_RADIUS: f32 = 0.08;

/// A segment within this many degrees of an axis is taken to be *meant* as
/// axis-aligned, and gets a constraint rather than just happening to look
/// straight. Matches the inference tolerance CAD sketchers expect.
const AXIS_SNAP_DEGREES: f32 = 5.0;

/// Snap radius in logical screen pixels, converted to world units with the
/// camera scale so snapping feels the same at every zoom level.
const SNAP_PIXELS: f32 = 10.0;

/// Below this radius a circle drag is treated as a stray click.
const MIN_RADIUS: f32 = 0.01;

/// A selection-driven operation the panel can ask for.
///
/// These are the geometry edits that are not gestures — they act on whatever
/// is already selected, so they belong on a panel rather than in a tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SketchOp {
    /// Round each selected corner with a tangent arc.
    Fillet {
        /// Arc radius, sketch units.
        radius: f32,
    },
    /// Cut each selected corner back to a straight line.
    Chamfer {
        /// How far back along each leg to cut.
        setback: f32,
    },
    /// Offset the selected chain by a distance, mitring the joints.
    Offset {
        /// Positive offsets to the left of the chain direction.
        distance: f32,
    },
    /// Demote the selected lines to construction geometry, or promote them
    /// back — reference lines you can snap to but that never become profile.
    ToggleConstruction,
    /// Delete the selected geometry and anything that referenced it.
    Delete,
}

/// The outcome of the last thing the author asked for, for the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStatus {
    /// Human-readable result.
    pub text: String,
    /// Whether it was a refusal rather than a success.
    pub error: bool,
}

/// The sketch being authored, plus everything the editor knows about it.
#[derive(Resource, Debug, Default)]
pub struct SketchSession {
    doc: SketchDoc,
    selection: SketchSelection,
    hover: Option<PickHit>,
    /// Remaining degrees of freedom from the last solve.
    dof: Option<i32>,
    /// Indices into `doc.constraints` the solver could not satisfy — drawn in
    /// red rather than silently dropped, which is the difference between a
    /// sketch you can debug and one that just refuses to move.
    failed: Vec<usize>,
    status: Option<SessionStatus>,
    tool: SketchTool,

    /// Line-chain in progress; `chain[0]` is where the loop closes.
    chain: Vec<SketchId>,
    /// Circle being dragged: centre point and live radius.
    circle: Option<(SketchId, f32)>,
    /// Arc being swept: the points collected so far (centre, then start).
    arc: Vec<SketchId>,
    /// The point being dragged, fed to the solver as a preference.
    drag: Option<SketchId>,
    /// First pick of a trim gesture — the thing to be cut.
    trim_target: Option<SketchId>,
    /// Set by the panel, consumed by the next `update`.
    commit_requested: bool,
    /// Whether newly drawn geometry is reference-only.
    construction: bool,
}

impl SketchSession {
    /// The sketch as it currently stands.
    pub fn doc(&self) -> &SketchDoc {
        &self.doc
    }

    /// What is currently selected.
    pub fn selection(&self) -> &SketchSelection {
        &self.selection
    }

    /// The snap candidate under the cursor, if any.
    pub fn hover(&self) -> Option<PickHit> {
        self.hover
    }

    /// Remaining degrees of freedom, once something has been solved.
    ///
    /// `Some(0)` means fully constrained — the state CAD users drive toward.
    pub fn dof(&self) -> Option<i32> {
        self.dof
    }

    /// Constraint indices the last solve could not satisfy.
    pub fn failed(&self) -> &[usize] {
        &self.failed
    }

    /// The result of the last panel action, for the status line.
    pub fn status(&self) -> Option<&SessionStatus> {
        self.status.as_ref()
    }

    /// Whether anything has been drawn.
    pub fn is_empty(&self) -> bool {
        self.doc.entities.is_empty() && self.doc.points.is_empty()
    }

    /// Whether new geometry is being drawn as construction.
    pub fn construction(&self) -> bool {
        self.construction
    }

    /// Draw subsequent geometry as construction, or stop doing so.
    pub fn set_construction(&mut self, on: bool) {
        self.construction = on;
    }

    /// Switch tools, abandoning whatever gesture was half-finished.
    ///
    /// The document and selection survive: switching from Line to Select must
    /// not throw away the sketch, only the dangling chain.
    pub fn set_tool(&mut self, tool: SketchTool) {
        if self.tool != tool {
            self.tool = tool;
            self.cancel_gesture();
        }
    }

    /// The active tool.
    pub fn tool(&self) -> SketchTool {
        self.tool
    }

    /// The constraints that could be applied to the current selection.
    pub fn applicable(&self) -> Vec<ConstraintKind> {
        edit::applicable(&self.doc, &self.selection)
    }

    /// Attach a constraint to the current selection and re-solve.
    ///
    /// Reports refusals through [`SketchSession::status`] rather than
    /// swallowing them — a constraint the editor offered but then dropped is
    /// worse than one it declined out loud.
    pub fn apply_constraint(&mut self, kind: ConstraintKind, value: Option<f32>) {
        match edit::apply(&mut self.doc, kind, &self.selection, value) {
            Ok(_) => {
                self.resolve();
                // A satisfied constraint has consumed the selection's meaning;
                // holding it would invite stacking a second one on the same
                // pair by accident.
                self.selection.clear();
                self.note(format!("{} applied", kind.label()), false);
            }
            Err(e) => self.note(e.to_string(), true),
        }
    }

    /// Remove the constraint at `index` and re-solve.
    pub fn remove_constraint(&mut self, index: usize) {
        if let Some(c) = edit::remove_constraint(&mut self.doc, index) {
            self.resolve();
            self.note(format!("removed {}", constraint_label(&c)), false);
        }
    }

    /// Run a selection-driven operation.
    pub fn run_op(&mut self, op: SketchOp) {
        let result = match op {
            SketchOp::Fillet { radius } => self.corner_op(radius, ops::fillet, "filleted"),
            SketchOp::Chamfer { setback } => self.corner_op(setback, ops::chamfer, "chamfered"),
            SketchOp::Offset { distance } => self.offset_op(distance),
            SketchOp::ToggleConstruction => self.construction_op(),
            SketchOp::Delete => self.delete_op(),
        };
        match result {
            Ok(text) => {
                self.resolve();
                self.note(text, false);
            }
            Err(text) => self.note(text, true),
        }
    }

    /// Fillet or chamfer every selected corner.
    fn corner_op(
        &mut self,
        amount: f32,
        f: fn(&mut SketchDoc, SketchId, f32) -> Result<SketchId, ops::OpError>,
        verb: &str,
    ) -> Result<String, String> {
        if self.selection.points.is_empty() {
            return Err("select a corner point first".into());
        }
        let corners = self.selection.points.clone();
        let mut done = 0;
        let mut last_error = None;
        for corner in corners {
            match f(&mut self.doc, corner, amount) {
                Ok(_) => done += 1,
                Err(e) => last_error = Some(e.to_string()),
            }
        }
        if done == 0 {
            return Err(last_error.unwrap_or_else(|| "no corner could be rounded".into()));
        }
        self.selection.clear();
        Ok(format!("{done} {verb}"))
    }

    fn offset_op(&mut self, distance: f32) -> Result<String, String> {
        if self.selection.entities.is_empty() {
            return Err("select a chain to offset".into());
        }
        let chain = self.selection.entities.clone();
        match ops::offset(&mut self.doc, &chain, distance) {
            Ok(made) => {
                self.selection.clear();
                Ok(format!("offset {} segment(s)", made.len()))
            }
            Err(e) => Err(e.to_string()),
        }
    }

    fn construction_op(&mut self) -> Result<String, String> {
        if self.selection.entities.is_empty() {
            return Err("select geometry to toggle".into());
        }
        let mut promoted = 0;
        let mut demoted = 0;
        for id in self.selection.entities.clone() {
            if self.doc.is_construction(id) {
                self.doc.unmark_construction(id);
                promoted += 1;
            } else {
                self.doc.mark_construction(id);
                demoted += 1;
            }
        }
        Ok(format!("{demoted} to reference, {promoted} to profile"))
    }

    fn delete_op(&mut self) -> Result<String, String> {
        if self.selection.is_empty() {
            return Err("nothing selected".into());
        }
        let entities = self.selection.entities.len();
        let points = self.selection.points.len();
        for id in self.selection.entities.clone() {
            self.doc.remove_entity(id);
        }
        for id in self.selection.points.clone() {
            self.doc.remove_point(id);
        }
        self.selection.clear();
        self.chain.clear();
        Ok(format!("deleted {entities} entity(s), {points} point(s)"))
    }

    /// Drop the current selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Ask for the sketch to be committed as a body on the next update.
    pub fn request_commit(&mut self) {
        self.commit_requested = true;
    }

    /// Whether the profile currently lowers to something committable.
    ///
    /// Lets the panel disable Commit rather than offering a button that will
    /// only produce an error.
    pub fn can_commit(&self) -> bool {
        lower::to_shape_with_origin(&self.doc).is_ok()
    }

    /// Discard the whole sketch.
    ///
    /// Leaving sketch mode must not leave a half-drawn chain waiting to
    /// reappear the next time the mode is entered.
    pub fn abandon(&mut self) {
        *self = Self {
            tool: self.tool,
            ..Self::default()
        };
    }

    /// Abandon the in-progress gesture, keeping the document.
    fn cancel_gesture(&mut self) {
        self.chain.clear();
        self.circle = None;
        self.arc.clear();
        self.drag = None;
        self.trim_target = None;
    }

    fn note(&mut self, text: impl Into<String>, error: bool) {
        self.status = Some(SessionStatus {
            text: text.into(),
            error,
        });
    }

    /// World position of a point after the last solve.
    fn at(&self, id: SketchId) -> Option<Vec2> {
        self.doc.point(id).map(|p| p.at)
    }

    /// Re-solve, keeping the last good geometry if the solver refuses.
    ///
    /// A rejected inference must never strand the draft: the solver leaves the
    /// document untouched on failure, so the worst case is that the geometry
    /// stays where it was drawn.
    fn resolve(&mut self) {
        self.resolve_dragging(None);
    }

    /// Re-solve with a drag preference, so the dragged point leads and the rest
    /// of the geometry follows its constraints.
    fn resolve_dragging(&mut self, drag: Option<SketchId>) {
        match solve::solve(&mut self.doc, drag) {
            Ok(outcome) => {
                self.dof = Some(outcome.dof);
                self.failed = outcome.failed;
                if !self.failed.is_empty() {
                    self.note(
                        format!("{} constraint(s) cannot be satisfied", self.failed.len()),
                        true,
                    );
                }
            }
            Err(e) => {
                self.dof = None;
                self.note(e.to_string(), true);
            }
        }
    }

    /// Mark newly created geometry as construction when that mode is on.
    fn tag(&mut self, id: SketchId) -> SketchId {
        if self.construction {
            self.doc.mark_construction(id);
        }
        id
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

    /// Resolve a click into a point to build from, attaching whatever
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

    /// Close the profile and hand back a body, clearing the sketch either way.
    fn commit(&mut self) -> Option<ToolCommit> {
        let doc = std::mem::take(&mut self.doc);
        let lowered = lower::to_shape_with_origin(&doc);
        self.abandon();

        match lowered {
            Ok((shape, origin)) => {
                shape.validate().ok()?;
                let mut record = new_body_record(shape, origin, 0.0);
                // The sketch rides along with the body: this is what makes the
                // body re-openable for constraint editing, and it is saved and
                // undone as one unit with the geometry it produced.
                record.sketch = Some(doc);
                Some(ToolCommit::SpawnBody(Box::new(record)))
            }
            Err(e) => {
                self.note(e.to_string(), true);
                None
            }
        }
    }
}

/// A short description of a constraint, for the constraint list.
fn constraint_label(c: &SketchConstraint) -> &'static str {
    match c {
        SketchConstraint::Coincident(..) => "coincident",
        SketchConstraint::Distance { .. } => "distance",
        SketchConstraint::Horizontal(_) => "horizontal",
        SketchConstraint::Vertical(_) => "vertical",
        SketchConstraint::Parallel(..) => "parallel",
        SketchConstraint::Perpendicular(..) => "perpendicular",
        SketchConstraint::EqualLength(..) => "equal length",
        SketchConstraint::PointOnLine { .. } => "point on line",
        SketchConstraint::Midpoint { .. } => "midpoint",
        SketchConstraint::Diameter { .. } => "diameter",
        SketchConstraint::EqualRadius(..) => "equal radius",
        SketchConstraint::Angle { .. } => "angle",
        SketchConstraint::PointOnCircle { .. } => "point on circle",
        SketchConstraint::PointLineDistance { .. } => "point-line distance",
        SketchConstraint::ArcLineTangent { .. } => "arc tangent",
        SketchConstraint::CubicLineTangent { .. } => "bezier tangent",
        SketchConstraint::CurveCurveTangent { .. } => "curve tangent",
        SketchConstraint::SymmetricAboutLine { .. } => "symmetric",
        SketchConstraint::LengthRatio { .. } => "length ratio",
        SketchConstraint::LengthDifference { .. } => "length difference",
        SketchConstraint::EqualAngle { .. } => "equal angle",
    }
}

/// A short description of a constraint, exposed for the constraint list.
#[must_use]
pub fn describe_constraint(c: &SketchConstraint) -> &'static str {
    constraint_label(c)
}

impl DraftTool for SketchSession {
    fn update(&mut self, ctx: &ToolContext) -> Option<ToolCommit> {
        if self.commit_requested {
            self.commit_requested = false;
            return self.commit();
        }
        if ctx.cancel {
            // Escape backs out of the gesture first and the selection second,
            // so an interrupted chain does not also cost the selection.
            if self.chain.is_empty() && self.circle.is_none() && self.arc.is_empty() {
                self.selection.clear();
            }
            self.cancel_gesture();
            return None;
        }

        // Hover tracking runs every frame, not just on press, so the preview
        // can show what a click would snap to before it happens.
        let tol = SNAP_PIXELS * ctx.cam_scale;
        self.hover = ctx.cursor.and_then(|c| pick::pick(&self.doc, c, tol));

        if ctx.confirm && self.tool == SketchTool::Line && self.chain.len() >= 3 {
            return self.close_chain();
        }

        match self.tool {
            SketchTool::Select => self.update_select(ctx, tol),
            SketchTool::Line => self.update_line(ctx, tol),
            SketchTool::Arc => self.update_arc(ctx, tol),
            SketchTool::Circle => self.update_circle(ctx, tol),
            SketchTool::Trim => self.update_trim(ctx),
        }
        None
    }

    fn drafting(&self) -> bool {
        !self.chain.is_empty()
            || self.circle.is_some()
            || !self.arc.is_empty()
            || self.drag.is_some()
    }

    /// The sketch is drawn whenever there is one, gesture or not — unlike the
    /// direct tools, whose preview exists only for the duration of a drag.
    fn wants_preview(&self) -> bool {
        self.drafting() || !self.is_empty()
    }

    fn preview(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        self.draw_document(out);
        self.draw_selection(out, ctx);
        self.draw_gesture(ctx, out);
        self.draw_hover(ctx, out);
    }
}

impl SketchSession {
    // -- gestures ----------------------------------------------------------

    /// Click to toggle selection, drag to move a point.
    ///
    /// Toggling rather than replacing is what lets a selection be built up to
    /// the two or four elements most constraints need, without a modifier key
    /// the tool seam does not carry. Clicking empty space clears.
    fn update_select(&mut self, ctx: &ToolContext, tol: f32) {
        match ctx.phase {
            GesturePhase::Pressed => {
                let Some(cursor) = ctx.cursor else { return };
                match pick::pick(&self.doc, cursor, tol) {
                    Some(hit) => match hit.target {
                        SketchTarget::Point(id) => {
                            self.selection.toggle_point(id);
                            self.drag = Some(id);
                        }
                        SketchTarget::Entity(id) => self.selection.toggle_entity(id),
                    },
                    None => self.selection.clear(),
                }
            }
            GesturePhase::Held => {
                // Live constrained dragging: the point follows the cursor as a
                // solver *preference*, so the rest of the sketch slides to keep
                // its constraints instead of the drag being refused.
                if let (Some(id), Some(cursor)) = (self.drag, ctx.cursor) {
                    if let Some(p) = self.doc.point_mut(id) {
                        p.at = cursor;
                    }
                    self.resolve_dragging(Some(id));
                }
            }
            GesturePhase::Released => self.drag = None,
            GesturePhase::Idle => {}
        }
    }

    fn update_line(&mut self, ctx: &ToolContext, tol: f32) {
        if ctx.phase != GesturePhase::Pressed {
            return;
        }
        let Some(p) = ctx.cursor else { return };

        let closes = self.chain.len() >= 3
            && self
                .chain
                .first()
                .and_then(|&f| self.at(f))
                .is_some_and(|f| f.distance(p) <= CLOSE_RADIUS);
        if closes {
            self.close_chain();
            return;
        }

        let id = self.point_for_click(p, tol);
        // Clicking the point we are already chaining from would make a
        // zero-length segment; ignore it rather than feeding the solver a
        // degenerate line.
        if self.chain.last() == Some(&id) {
            return;
        }
        if let Some(&prev) = self.chain.last() {
            let line = self.doc.add_line(prev, id);
            self.tag(line);
            if let Some(c) = self.infer_axis(prev, id, line) {
                self.doc.constrain(c);
            }
            self.chain.push(id);
            self.resolve();
        } else {
            self.chain.push(id);
        }
    }

    /// Join the chain's last point back to its first.
    ///
    /// Returns `None` always — closing a loop finishes the *chain*, not the
    /// sketch. The author may keep drawing more loops (holes, say) and commits
    /// the whole document explicitly.
    fn close_chain(&mut self) -> Option<ToolCommit> {
        if let (Some(&last), Some(&first)) = (self.chain.last(), self.chain.first())
            && last != first
        {
            let line = self.doc.add_line(last, first);
            self.tag(line);
            if let Some(c) = self.infer_axis(last, first, line) {
                self.doc.constrain(c);
            }
            self.resolve();
        }
        self.chain.clear();
        None
    }

    /// Three clicks: centre, start, end.
    fn update_arc(&mut self, ctx: &ToolContext, tol: f32) {
        if ctx.phase != GesturePhase::Pressed {
            return;
        }
        let Some(p) = ctx.cursor else { return };
        let id = self.point_for_click(p, tol);
        if self.arc.last() == Some(&id) {
            return;
        }
        self.arc.push(id);
        if self.arc.len() == 3 {
            let (c, s, e) = (self.arc[0], self.arc[1], self.arc[2]);
            let arc = self.doc.add_arc(c, s, e);
            self.tag(arc);
            self.arc.clear();
            self.resolve();
        }
    }

    fn update_circle(&mut self, ctx: &ToolContext, tol: f32) {
        match ctx.phase {
            GesturePhase::Pressed => {
                let Some(p) = ctx.cursor else { return };
                let center = self.point_for_click(p, tol);
                self.circle = Some((center, 0.0));
            }
            GesturePhase::Held => {
                if let (Some((center, _)), Some(p)) = (self.circle, ctx.cursor)
                    && let Some(c) = self.at(center)
                {
                    self.circle = Some((center, c.distance(p)));
                }
            }
            GesturePhase::Released => {
                let Some((center, r)) = self.circle.take() else {
                    return;
                };
                if r < MIN_RADIUS {
                    // A stray click leaves a stray point behind otherwise.
                    self.doc.remove_point(center);
                    return;
                }
                let circle = self.doc.add_circle(center, r);
                self.tag(circle);
                self.resolve();
            }
            GesturePhase::Idle => {}
        }
    }

    /// Two clicks: the thing to cut, then the boundary to cut it against.
    ///
    /// Trimming back and extending forward are the same gesture — which end
    /// moves is decided by where the first click landed, so there is no
    /// separate Extend tool to hunt for.
    fn update_trim(&mut self, ctx: &ToolContext) {
        if ctx.phase != GesturePhase::Pressed {
            return;
        }
        let (Some(cursor), Some(hit)) = (ctx.cursor, self.hover) else {
            return;
        };
        let SketchTarget::Entity(id) = hit.target else {
            return;
        };
        match self.trim_target.take() {
            None => {
                self.trim_target = Some(id);
                self.note("now click the boundary to trim against", false);
            }
            Some(target) if target == id => {
                self.note("pick a different entity as the boundary", true);
            }
            Some(target) => match ops::trim(&mut self.doc, target, id, cursor) {
                Ok(()) => {
                    self.resolve();
                    self.note("trimmed", false);
                }
                Err(e) => self.note(e.to_string(), true),
            },
        }
    }

    // -- preview -----------------------------------------------------------

    /// Draw the committed sketch geometry, dimming construction lines.
    ///
    /// Reference geometry reads differently from profile geometry because it
    /// *is* different — it will not become part of the body — and an author
    /// who cannot tell them apart cannot trust either.
    fn draw_document(&self, out: &mut ToolPreview) {
        for e in &self.doc.entities {
            let reference = self.doc.is_construction(e.id());
            let color = if reference {
                css::SLATE_GRAY.with_alpha(0.7)
            } else {
                css::AQUAMARINE
            };
            self.draw_entity(e, color, out);
        }
    }

    fn draw_entity(&self, e: &SketchEntity, color: Srgba, out: &mut ToolPreview) {
        match *e {
            SketchEntity::Line { a, b, .. } => {
                if let (Some(pa), Some(pb)) = (self.at(a), self.at(b)) {
                    out.line(pa, pb, color);
                }
            }
            SketchEntity::Circle { center, radius, .. } => {
                if let Some(c) = self.at(center) {
                    out.circle(c, radius, color);
                }
            }
            SketchEntity::Arc {
                center, start, end, ..
            } => {
                if let (Some(c), Some(s), Some(f)) = (self.at(center), self.at(start), self.at(end))
                {
                    out.polyline(arc_points(c, s, f), color);
                }
            }
            SketchEntity::Cubic {
                start,
                start_control,
                end_control,
                end,
                ..
            } => {
                let pts = [start, start_control, end_control, end].map(|p| self.at(p));
                if let [Some(p0), Some(c0), Some(c1), Some(p1)] = pts {
                    out.polyline(cubic_points(p0, c0, c1, p1), color);
                }
            }
        }
    }

    /// Highlight what is selected, and mark points so they can be grabbed.
    fn draw_selection(&self, out: &mut ToolPreview, ctx: &ToolContext) {
        let r = ctx.cam_scale * 3.0;
        for p in &self.doc.points {
            let selected = self.selection.points.contains(&p.id);
            let color = if selected {
                css::ORANGE
            } else if p.fixed {
                // A pinned point is a fact about the sketch worth seeing.
                css::TOMATO.with_alpha(0.8)
            } else {
                css::AQUAMARINE.with_alpha(0.5)
            };
            out.circle(p.at, if selected { r * 1.8 } else { r }, color);
        }
        for e in &self.doc.entities {
            if self.selection.entities.contains(&e.id()) {
                self.draw_entity(e, css::ORANGE, out);
            }
        }
        // Constraints the solver rejected: the sketch is telling you which
        // relationship it cannot honour, rather than just refusing to move.
        for &index in &self.failed {
            if let Some(c) = self.doc.constraints.get(index) {
                self.draw_failed(*c, out);
            }
        }
    }

    /// Redraw whatever a failed constraint names, in the failure colour.
    fn draw_failed(&self, c: SketchConstraint, out: &mut ToolPreview) {
        for id in c.operands() {
            if let Some(e) = self.doc.entities.iter().find(|e| e.id() == id) {
                self.draw_entity(e, css::RED, out);
            } else if let Some(p) = self.doc.point(id) {
                out.circle(p.at, 0.02, css::RED);
            }
        }
    }

    /// The in-progress gesture, drawn ahead of the cursor.
    fn draw_gesture(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        let pts: Vec<Vec2> = self.chain.iter().filter_map(|&id| self.at(id)).collect();
        if pts.len() >= 2 {
            out.polyline(pts.clone(), css::YELLOW);
        }
        if let (Some(&last), Some(p)) = (self.chain.last(), ctx.cursor)
            && let Some(a) = self.at(last)
        {
            out.line(a, p, css::YELLOW.with_alpha(0.5));
        }
        // The closing hint, so it is obvious where the loop completes.
        if pts.len() >= 3
            && let (Some(first), Some(p)) = (pts.first().copied(), ctx.cursor)
            && first.distance(p) <= CLOSE_RADIUS
        {
            out.line(p, first, css::LIME);
        }

        if let Some((center, r)) = self.circle
            && r >= MIN_RADIUS
            && let Some(c) = self.at(center)
        {
            out.circle(c, r, css::YELLOW);
        }

        // Arc in progress: show the radius leg, then the sweep so far.
        if let (Some(&c), Some(p)) = (self.arc.first(), ctx.cursor)
            && let Some(cp) = self.at(c)
        {
            match self.arc.get(1).and_then(|&s| self.at(s)) {
                Some(sp) => {
                    out.line(cp, sp, css::YELLOW.with_alpha(0.5));
                    out.polyline(arc_points(cp, sp, p), css::YELLOW);
                }
                None => out.line(cp, p, css::YELLOW.with_alpha(0.5)),
            }
        }

        // Trim: the entity awaiting a boundary.
        if let Some(id) = self.trim_target
            && let Some(e) = self.doc.entities.iter().find(|e| e.id() == id)
        {
            self.draw_entity(e, css::MAGENTA, out);
        }
    }

    /// The snap marker: what a click would attach to.
    fn draw_hover(&self, ctx: &ToolContext, out: &mut ToolPreview) {
        let Some(hit) = self.hover else { return };
        let color = match hit.kind {
            SnapKind::Point => css::ORANGE,
            SnapKind::Midpoint => css::YELLOW,
            SnapKind::Center => css::MAGENTA,
            SnapKind::OnEntity => css::AQUA,
        };
        out.circle(hit.at, ctx.cam_scale * 5.0, color);
        // Highlight the whole entity under the cursor, not only the snap dot —
        // the Plasticity habit of making it obvious what you are about to act
        // on before you commit to it.
        if let SketchTarget::Entity(id) = hit.target
            && let Some(e) = self.doc.entities.iter().find(|e| e.id() == id)
        {
            self.draw_entity(e, color.with_alpha(0.6), out);
        }
    }
}

/// How many segments an arc preview is drawn with.
const ARC_SEGMENTS: usize = 32;

/// Flatten an arc from `start` to `end` about `center`, the short way round.
fn arc_points(center: Vec2, start: Vec2, end: Vec2) -> Vec<Vec2> {
    let r = center.distance(start);
    if r < f32::EPSILON {
        return vec![start, end];
    }
    let a0 = (start - center).to_angle();
    let a1 = (end - center).to_angle();
    let mut sweep = a1 - a0;
    // Normalise into (-pi, pi] so the preview takes the short way round, which
    // is what the click sequence implies.
    while sweep > std::f32::consts::PI {
        sweep -= std::f32::consts::TAU;
    }
    while sweep <= -std::f32::consts::PI {
        sweep += std::f32::consts::TAU;
    }
    (0..=ARC_SEGMENTS)
        .map(|i| {
            let t = i as f32 / ARC_SEGMENTS as f32;
            let a = a0 + sweep * t;
            center + Vec2::from_angle(a) * r
        })
        .collect()
}

/// Flatten a cubic for preview, matching the document's own discretization.
fn cubic_points(p0: Vec2, c0: Vec2, c1: Vec2, p1: Vec2) -> Vec<Vec2> {
    use gradiance_sketch::doc::{CUBIC_SEGMENTS, cubic_at};
    (0..=CUBIC_SEGMENTS)
        .map(|i| cubic_at(p0, c0, c1, p1, i as f32 / CUBIC_SEGMENTS as f32))
        .collect()
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

    /// A session with the given tool selected.
    fn session(tool: SketchTool) -> SketchSession {
        let mut s = SketchSession::default();
        s.set_tool(tool);
        s
    }

    /// Click a sequence of points, returning any commit the last click made.
    fn click_all(s: &mut SketchSession, pts: &[Vec2]) -> Option<ToolCommit> {
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mut last = None;
        for p in pts {
            last = s.update(&ctx(GesturePhase::Pressed, Some(*p), &gc, &sc));
        }
        last
    }

    /// Press, hold and release across a drag.
    fn drag(s: &mut SketchSession, from: Vec2, to: Vec2) {
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        s.update(&ctx(GesturePhase::Pressed, Some(from), &gc, &sc));
        s.update(&ctx(GesturePhase::Held, Some(to), &gc, &sc));
        s.update(&ctx(GesturePhase::Released, Some(to), &gc, &sc));
    }

    /// A closed unit square, drawn with the line tool.
    fn square() -> SketchSession {
        let mut s = session(SketchTool::Line);
        click_all(
            &mut s,
            &[
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
        );
        s.close_chain();
        s
    }

    #[test]
    fn a_near_horizontal_segment_is_solved_exactly_horizontal() {
        let mut s = session(SketchTool::Line);
        // Second point is 2 degrees off horizontal — within the inference
        // tolerance, so it should be *made* horizontal, not left as drawn.
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.035)]);

        let pts: Vec<Vec2> = s.chain.iter().filter_map(|&id| s.at(id)).collect();
        assert_eq!(pts.len(), 2);
        assert!(
            (pts[1].y - pts[0].y).abs() < 1e-4,
            "expected the solver to flatten the segment, got {pts:?}"
        );
    }

    #[test]
    fn a_clearly_diagonal_segment_is_left_alone() {
        let mut s = session(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)]);

        let pts: Vec<Vec2> = s.chain.iter().filter_map(|&id| s.at(id)).collect();
        assert!(
            (pts[1] - Vec2::new(1.0, 1.0)).length() < 1e-4,
            "a deliberate diagonal must not be snapped square, got {pts:?}"
        );
    }

    #[test]
    fn clicking_an_existing_point_reuses_its_identity() {
        let mut s = session(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(0.5, 0.7)]);
        let before = s.doc.points.len();

        // Start a new chain landing on the first point.
        s.chain.clear();
        click_all(&mut s, &[Vec2::new(0.0, 0.0)]);

        assert_eq!(
            s.doc.points.len(),
            before,
            "a snapped click must share the point, not add a coincident twin"
        );
    }

    #[test]
    fn every_tool_draws_into_the_same_document() {
        // The reason the session exists: a circle and a line have to be able to
        // end up in one document, or no constraint can ever name both.
        let mut s = session(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]);
        let lines = s.doc.entities.len();

        s.set_tool(SketchTool::Circle);
        drag(&mut s, Vec2::new(3.0, 3.0), Vec2::new(3.5, 3.0));

        assert_eq!(s.doc.entities.len(), lines + 1);
        assert!(
            matches!(s.doc.entities.last(), Some(SketchEntity::Circle { .. })),
            "the circle joined the line's document rather than a private one"
        );
    }

    #[test]
    fn a_stray_circle_click_leaves_no_orphan_point() {
        let mut s = session(SketchTool::Circle);
        let before = s.doc.points.len();
        drag(&mut s, Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0));

        assert_eq!(s.doc.entities.len(), 0, "no circle from a zero drag");
        assert_eq!(
            s.doc.points.len(),
            before,
            "the centre point must not be left behind"
        );
    }

    #[test]
    fn three_clicks_make_an_arc() {
        let mut s = session(SketchTool::Arc);
        click_all(
            &mut s,
            &[
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 0.0),
                Vec2::new(0.0, 1.0),
            ],
        );
        assert!(
            matches!(s.doc.entities.first(), Some(SketchEntity::Arc { .. })),
            "centre, start, end should have produced an arc: {:?}",
            s.doc.entities
        );
        assert!(s.arc.is_empty(), "the gesture resets for the next arc");
    }

    #[test]
    fn selecting_toggles_rather_than_replaces() {
        // Most constraints need two elements, and the tool seam carries no
        // modifier key, so a second click has to add rather than replace.
        let mut s = square();
        s.set_tool(SketchTool::Select);

        click_all(&mut s, &[Vec2::new(0.0, 0.0)]);
        assert_eq!(s.selection.points.len(), 1);

        click_all(&mut s, &[Vec2::new(1.0, 1.0)]);
        assert_eq!(s.selection.points.len(), 2, "the second click added");

        click_all(&mut s, &[Vec2::new(1.0, 1.0)]);
        assert_eq!(s.selection.points.len(), 1, "clicking again removed it");
    }

    #[test]
    fn clicking_empty_space_clears_the_selection() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        click_all(&mut s, &[Vec2::new(0.0, 0.0)]);
        assert!(!s.selection.is_empty());

        click_all(&mut s, &[Vec2::new(50.0, 50.0)]);
        assert!(s.selection.is_empty());
    }

    #[test]
    fn dragging_a_point_moves_it_and_re_solves() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        let corner = s.doc.points[2].id;
        let before = s.at(corner).expect("corner exists");

        drag(&mut s, before, before + Vec2::new(0.4, 0.4));

        let after = s.at(corner).expect("corner still exists");
        assert!(
            after.distance(before) > 0.1,
            "the dragged point should have followed the cursor: {before:?} -> {after:?}"
        );
        assert!(s.dof.is_some(), "the drag re-solved the sketch");
    }

    #[test]
    fn switching_tools_keeps_the_sketch_but_drops_the_gesture() {
        let mut s = session(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]);
        assert!(!s.chain.is_empty());

        s.set_tool(SketchTool::Select);

        assert!(s.chain.is_empty(), "the dangling chain is abandoned");
        assert_eq!(s.doc.entities.len(), 1, "the drawn segment survives");
    }

    #[test]
    fn a_constraint_applies_to_the_selection_and_reports_itself() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        // Two opposite edges: parallel is offered for a pair of lines.
        let (a, b) = (s.doc.entities[0].id(), s.doc.entities[2].id());
        s.selection.toggle_entity(a);
        s.selection.toggle_entity(b);

        assert!(s.applicable().contains(&ConstraintKind::Parallel));
        let before = s.doc.constraints.len();
        s.apply_constraint(ConstraintKind::Parallel, None);

        assert_eq!(s.doc.constraints.len(), before + 1);
        assert!(s.selection.is_empty(), "a satisfied selection is consumed");
        assert!(s.status().is_some_and(|st| !st.error));
    }

    #[test]
    fn an_inapplicable_constraint_is_refused_out_loud() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        s.selection.toggle_point(s.doc.points[0].id);

        // A lone point cannot be a diameter.
        s.apply_constraint(ConstraintKind::Diameter, Some(1.0));

        assert!(
            s.status().is_some_and(|st| st.error),
            "a refusal has to surface, not vanish"
        );
    }

    #[test]
    fn a_dimension_drives_the_geometry() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        let (a, b) = (s.doc.points[0].id, s.doc.points[1].id);
        s.selection.toggle_point(a);
        s.selection.toggle_point(b);

        s.apply_constraint(ConstraintKind::Distance, Some(3.0));

        let (pa, pb) = (s.at(a).expect("a"), s.at(b).expect("b"));
        assert!(
            (pa.distance(pb) - 3.0).abs() < 1e-3,
            "the dimension should have moved the geometry, got {}",
            pa.distance(pb)
        );
    }

    #[test]
    fn removing_a_constraint_re_solves() {
        let mut s = square();
        let before = s.doc.constraints.len();
        assert!(before > 0, "the square picked up axis constraints as drawn");

        s.remove_constraint(0);

        assert_eq!(s.doc.constraints.len(), before - 1);
        assert!(s.status().is_some_and(|st| !st.error));
    }

    #[test]
    fn reference_geometry_toggles_and_leaves_the_profile() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        let edge = s.doc.entities[0].id();
        s.selection.toggle_entity(edge);

        s.run_op(SketchOp::ToggleConstruction);
        assert!(s.doc.is_construction(edge));

        // The selection deliberately survives this op — unlike a constraint,
        // which consumes what it was applied to, marking geometry as reference
        // is something you want to be able to undo with a second click.
        assert!(
            s.selection.entities.contains(&edge),
            "the selection survives a construction toggle"
        );
        s.run_op(SketchOp::ToggleConstruction);
        assert!(!s.doc.is_construction(edge), "and back again");
    }

    #[test]
    fn deleting_a_point_takes_its_edges_with_it() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        s.selection.toggle_point(s.doc.points[0].id);

        s.run_op(SketchOp::Delete);

        assert_eq!(
            s.doc.entities.len(),
            2,
            "the two edges meeting at that corner went with it"
        );
        assert!(s.selection.is_empty());
    }

    #[test]
    fn an_op_with_nothing_selected_says_so() {
        let mut s = square();
        s.run_op(SketchOp::Fillet { radius: 0.1 });
        assert!(s.status().is_some_and(|st| st.error));
    }

    #[test]
    fn a_filleted_corner_gains_an_arc() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        let corner = s.doc.points[1].id;
        s.selection.toggle_point(corner);
        let before = s.doc.entities.len();

        s.run_op(SketchOp::Fillet { radius: 0.2 });

        assert!(
            s.doc.entities.len() > before,
            "a fillet inserts a tangent arc: {:?}",
            s.status()
        );
        assert!(s.status().is_some_and(|st| !st.error), "{:?}", s.status());
    }

    #[test]
    fn a_closed_profile_commits_to_a_body_carrying_its_sketch() {
        let mut s = square();
        assert!(s.can_commit());

        s.request_commit();
        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let commit = s.update(&ctx(GesturePhase::Idle, None, &gc, &sc));

        match commit {
            Some(ToolCommit::SpawnBody(record)) => assert!(
                record.sketch.is_some(),
                "the sketch rides with the body so it can be reopened"
            ),
            other => panic!("expected a body, got {other:?}"),
        }
        assert!(s.is_empty(), "committing clears the session");
    }

    #[test]
    fn an_open_profile_cannot_be_committed() {
        let mut s = session(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]);
        assert!(
            !s.can_commit(),
            "a bare segment is not a profile, and the panel disables Commit on this"
        );
    }

    #[test]
    fn escape_backs_out_of_the_gesture_before_the_selection() {
        let mut s = square();
        s.set_tool(SketchTool::Select);
        s.selection.toggle_point(s.doc.points[0].id);
        s.set_tool(SketchTool::Line);
        click_all(&mut s, &[Vec2::new(5.0, 5.0), Vec2::new(6.0, 5.0)]);

        let (gc, sc) = (GestureConstraints::default(), SnapConfig::default());
        let mut cancel = ctx(GesturePhase::Idle, None, &gc, &sc);
        cancel.cancel = true;

        s.update(&cancel);
        assert!(s.chain.is_empty(), "the chain went first");
        assert!(!s.selection.is_empty(), "the selection survived");

        s.update(&cancel);
        assert!(s.selection.is_empty(), "a second escape clears it");
    }

    #[test]
    fn reference_mode_marks_what_it_draws() {
        let mut s = session(SketchTool::Line);
        s.set_construction(true);
        click_all(&mut s, &[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)]);

        let edge = s.doc.entities[0].id();
        assert!(s.doc.is_construction(edge));
    }
}
