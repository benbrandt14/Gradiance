//! The authored sketch document.
//!
//! A [`SketchDoc`] is *authored* state: it is what the person drew and the
//! relationships they asked for. It is captured in undo records and written to
//! the save file. The geometry the solver settles on is stored back into the
//! document's points — the solver refines authored state rather than producing
//! a separate derived copy.
//!
//! This module is deliberately **dimension-agnostic**. SolveSpace is a 3D
//! solver and the bridge in [`crate::solve`](mod@crate::solve) builds a real workplane, so a
//! future 3D construction plane is a different workplane rather than a
//! different document format. Nothing here mentions `ShapeDef`; the only
//! 2D-specific step lives in [`crate::lower`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Identity of a point, entity, or constraint *within* one sketch.
///
/// Stable across edits and solves, so tools and undo records can name a vertex
/// over time. Deliberately **not** the solver's handle: SolveSpace handles are
/// ephemeral, minted fresh on every solve and never stored (the same reasoning
/// that keeps `StableId` rather than `Entity` in save files).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Reflect,
)]
pub struct SketchId(pub u32);

/// A point in the sketch, in workplane coordinates (metres).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct SketchPoint {
    /// Identity within the sketch.
    pub id: SketchId,
    /// Position in workplane `(u, v)` coordinates, in metres.
    pub at: Vec2,
    /// Whether the solver may move this point.
    ///
    /// Fixed points act as anchors — a hard constraint. This is authored
    /// intent, distinct from the transient drag *hint* applied during a
    /// gesture, which only biases the solver.
    #[serde(default)]
    pub fixed: bool,
}

/// A geometric element, referring to its points by [`SketchId`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SketchEntity {
    /// Straight segment between two points.
    Line {
        /// Identity within the sketch.
        id: SketchId,
        /// Start point.
        a: SketchId,
        /// End point.
        b: SketchId,
    },
    /// Circular arc, counter-clockwise from `start` to `end` about `center`.
    Arc {
        /// Identity within the sketch.
        id: SketchId,
        /// Centre point.
        center: SketchId,
        /// Start point.
        start: SketchId,
        /// End point.
        end: SketchId,
    },
    /// Cubic bezier: two endpoints with a control point apiece.
    ///
    /// SolveSpace's `Cubic` is a single bezier segment rather than a general
    /// NURBS, so tangency is the strongest continuity available — enough for
    /// smooth joins, short of curvature matching.
    Cubic {
        /// Identity within the sketch.
        id: SketchId,
        /// Start point.
        start: SketchId,
        /// Control point leaving `start`.
        start_control: SketchId,
        /// Control point entering `end`.
        end_control: SketchId,
        /// End point.
        end: SketchId,
    },
    /// Full circle about `center`.
    ///
    /// The radius is a solver parameter, so it can be driven by a
    /// [`SketchConstraint::Diameter`] or left free.
    Circle {
        /// Identity within the sketch.
        id: SketchId,
        /// Centre point.
        center: SketchId,
        /// Radius in metres.
        radius: f32,
    },
}

impl SketchEntity {
    /// Identity of this entity within the sketch.
    #[must_use]
    pub fn id(&self) -> SketchId {
        match *self {
            SketchEntity::Line { id, .. }
            | SketchEntity::Arc { id, .. }
            | SketchEntity::Cubic { id, .. }
            | SketchEntity::Circle { id, .. } => id,
        }
    }
}

/// Evaluate a cubic bezier at `t` in `[0, 1]`.
///
/// Shared by lowering and hit-testing so a bezier is discretized the same way
/// wherever it is consumed — a curve that picks differently from how it draws
/// is a curve users cannot click.
#[must_use]
pub fn cubic_at(p0: Vec2, c0: Vec2, c1: Vec2, p1: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u * u) + c0 * (3.0 * u * u * t) + c1 * (3.0 * u * t * t) + p1 * (t * t * t)
}

/// Samples used to discretize one bezier segment.
pub const CUBIC_SEGMENTS: usize = 24;

/// A relationship the solver must satisfy.
///
/// Each variant maps onto exactly one SolveSpace constraint; the mapping lives
/// in [`crate::solve`](mod@crate::solve).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub enum SketchConstraint {
    /// Two points occupy the same location.
    Coincident(SketchId, SketchId),
    /// Two points are a fixed distance apart.
    Distance {
        /// First point.
        a: SketchId,
        /// Second point.
        b: SketchId,
        /// Required separation in metres.
        d: f32,
    },
    /// A line is horizontal in workplane coordinates.
    Horizontal(SketchId),
    /// A line is vertical in workplane coordinates.
    Vertical(SketchId),
    /// Two lines are parallel.
    Parallel(SketchId, SketchId),
    /// Two lines meet at a right angle.
    Perpendicular(SketchId, SketchId),
    /// Two lines have equal length.
    EqualLength(SketchId, SketchId),
    /// A point lies on a line.
    PointOnLine {
        /// The point.
        point: SketchId,
        /// The line it lies on.
        line: SketchId,
    },
    /// A point lies at the midpoint of a line.
    Midpoint {
        /// The point.
        point: SketchId,
        /// The line it bisects.
        line: SketchId,
    },
    /// A circle or arc has a fixed diameter.
    Diameter {
        /// The circle or arc.
        entity: SketchId,
        /// Required diameter in metres.
        d: f32,
    },
    /// Two circles or arcs have equal radius.
    EqualRadius(SketchId, SketchId),
    /// Two lines meet at a fixed angle, in degrees.
    Angle {
        /// First line.
        a: SketchId,
        /// Second line.
        b: SketchId,
        /// Required angle in degrees.
        degrees: f32,
    },
    /// A point lies on a circle or arc.
    PointOnCircle {
        /// The point.
        point: SketchId,
        /// The circle or arc it lies on.
        circle: SketchId,
    },
    /// A point sits a fixed distance from a line.
    PointLineDistance {
        /// The point.
        point: SketchId,
        /// The line measured from.
        line: SketchId,
        /// Required distance in metres.
        d: f32,
    },
    /// An arc meets a line tangentially.
    ArcLineTangent {
        /// The arc.
        arc: SketchId,
        /// The line it is tangent to.
        line: SketchId,
        /// Whether the tangency is at the arc's end rather than its start.
        at_end: bool,
    },
    /// A cubic bezier meets a line tangentially.
    CubicLineTangent {
        /// The bezier.
        cubic: SketchId,
        /// The line it is tangent to.
        line: SketchId,
        /// Whether the tangency is at the bezier's end rather than its start.
        at_end: bool,
    },
    /// Two curves (arc or bezier) meet tangentially.
    ///
    /// This is the smooth-join condition between spline segments — the
    /// strongest continuity SolveSpace offers, short of curvature matching.
    CurveCurveTangent {
        /// First curve.
        a: SketchId,
        /// Second curve.
        b: SketchId,
        /// Whether the first curve joins at its end rather than its start.
        a_at_end: bool,
        /// Whether the second curve joins at its end rather than its start.
        b_at_end: bool,
    },
    /// Two points are mirror images about a line.
    SymmetricAboutLine {
        /// First point.
        a: SketchId,
        /// Second point.
        b: SketchId,
        /// The mirror line.
        line: SketchId,
    },
    /// Two lines' lengths hold a fixed ratio (`a` / `b`).
    LengthRatio {
        /// Numerator line.
        a: SketchId,
        /// Denominator line.
        b: SketchId,
        /// Required ratio.
        ratio: f32,
    },
    /// Two lines' lengths differ by a fixed amount (`a` - `b`).
    LengthDifference {
        /// First line.
        a: SketchId,
        /// Second line.
        b: SketchId,
        /// Required difference in metres.
        difference: f32,
    },
    /// The angle between `a` and `b` equals the angle between `c` and `d`.
    EqualAngle {
        /// First line of the first pair.
        a: SketchId,
        /// Second line of the first pair.
        b: SketchId,
        /// First line of the second pair.
        c: SketchId,
        /// Second line of the second pair.
        d: SketchId,
    },
}

/// A constrained 2D sketch.
///
/// Retained **only** for bodies authored in sketch mode. Bodies drawn with the
/// direct tools carry no `SketchDoc`.
#[derive(Component, Debug, Clone, Default, PartialEq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct SketchDoc {
    /// Every point in the sketch.
    pub points: Vec<SketchPoint>,
    /// Every geometric element.
    pub entities: Vec<SketchEntity>,
    /// Every relationship the solver must satisfy.
    pub constraints: Vec<SketchConstraint>,
    /// Entities that are reference-only: solved like any other geometry, drawn
    /// dimmed, and excluded from the committed profile.
    #[serde(default)]
    pub construction: Vec<SketchId>,
    /// Source of fresh [`SketchId`]s. Monotonic, so ids are never reused even
    /// after deletion — a reused id would silently re-target constraints.
    #[serde(default)]
    next_id: u32,
}

impl SketchDoc {
    /// An empty sketch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint an identity that no element of this sketch has used.
    pub fn fresh_id(&mut self) -> SketchId {
        let id = SketchId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Add a point at `at`, returning its identity.
    pub fn add_point(&mut self, at: Vec2) -> SketchId {
        let id = self.fresh_id();
        self.points.push(SketchPoint {
            id,
            at,
            fixed: false,
        });
        id
    }

    /// Add a line between two existing points, returning its identity.
    pub fn add_line(&mut self, a: SketchId, b: SketchId) -> SketchId {
        let id = self.fresh_id();
        self.entities.push(SketchEntity::Line { id, a, b });
        id
    }

    /// Add a counter-clockwise arc, returning its identity.
    pub fn add_arc(&mut self, center: SketchId, start: SketchId, end: SketchId) -> SketchId {
        let id = self.fresh_id();
        self.entities.push(SketchEntity::Arc {
            id,
            center,
            start,
            end,
        });
        id
    }

    /// Add a cubic bezier, returning its identity.
    pub fn add_cubic(
        &mut self,
        start: SketchId,
        start_control: SketchId,
        end_control: SketchId,
        end: SketchId,
    ) -> SketchId {
        let id = self.fresh_id();
        self.entities.push(SketchEntity::Cubic {
            id,
            start,
            start_control,
            end_control,
            end,
        });
        id
    }

    /// Add a full circle, returning its identity.
    pub fn add_circle(&mut self, center: SketchId, radius: f32) -> SketchId {
        let id = self.fresh_id();
        self.entities
            .push(SketchEntity::Circle { id, center, radius });
        id
    }

    /// Record a constraint.
    pub fn constrain(&mut self, c: SketchConstraint) {
        self.constraints.push(c);
    }

    /// Look up a point by identity.
    #[must_use]
    pub fn point(&self, id: SketchId) -> Option<&SketchPoint> {
        self.points.iter().find(|p| p.id == id)
    }

    /// Look up a point mutably by identity.
    pub fn point_mut(&mut self, id: SketchId) -> Option<&mut SketchPoint> {
        self.points.iter_mut().find(|p| p.id == id)
    }

    /// Look up an entity by identity.
    #[must_use]
    pub fn entity(&self, id: SketchId) -> Option<&SketchEntity> {
        self.entities.iter().find(|e| e.id() == id)
    }

    /// Whether `id` is reference-only geometry.
    #[must_use]
    pub fn is_construction(&self, id: SketchId) -> bool {
        self.construction.contains(&id)
    }

    /// Mark an entity as reference-only.
    pub fn mark_construction(&mut self, id: SketchId) {
        if !self.is_construction(id) {
            self.construction.push(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_never_reused() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::ZERO);
        let b = doc.add_point(Vec2::X);
        let line = doc.add_line(a, b);
        assert_ne!(a, b);
        assert_ne!(b, line);

        // Deleting geometry must not free the id for reuse: a recycled id would
        // silently re-target any constraint still naming it.
        doc.points.retain(|p| p.id != b);
        let c = doc.add_point(Vec2::Y);
        assert_ne!(c, b);
        assert_ne!(c, a);
    }

    #[test]
    fn lookups_resolve_by_identity_not_position() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::new(1.0, 2.0));
        let b = doc.add_point(Vec2::new(3.0, 4.0));
        // Removing the *first* point must not shift what `b` resolves to.
        doc.points.retain(|p| p.id != a);
        assert_eq!(doc.point(b).map(|p| p.at), Some(Vec2::new(3.0, 4.0)));
        assert!(doc.point(a).is_none());
    }

    #[test]
    fn construction_geometry_is_tracked() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::ZERO);
        let b = doc.add_point(Vec2::X);
        let line = doc.add_line(a, b);
        assert!(!doc.is_construction(line));
        doc.mark_construction(line);
        doc.mark_construction(line);
        assert!(doc.is_construction(line));
        assert_eq!(
            doc.construction.len(),
            1,
            "marking twice must not duplicate"
        );
    }
}
