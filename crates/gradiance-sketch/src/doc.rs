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
    /// The body this point is pinned to, if it was placed on one.
    ///
    /// An **opaque foreign key**: this crate never dereferences it, and could
    /// not — it has no physics edge and no way to reach an entity. To the
    /// solver an anchored point is just a point. Resolving the id, reading the
    /// body's pose, and turning an anchored line into a joint all happen in
    /// `interaction`/`command`, which already depend on both sides. That is
    /// what lets a sketch reference the world without the sketch layer
    /// learning what the world is.
    #[serde(default)]
    pub anchor: Option<gradiance_core::ids::StableId>,
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
            | SketchEntity::Circle { id, .. } => id,
        }
    }
}

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
    /// Two arcs meet tangentially.
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
}

impl SketchConstraint {
    /// Every piece of geometry this constraint names.
    ///
    /// One place that knows the operand shape of each variant, rather than the
    /// same match written once for deletion and again for drawing failures.
    /// The match is exhaustive, so a new constraint variant has to declare what
    /// it references before it will compile.
    #[must_use]
    pub fn operands(&self) -> Vec<SketchId> {
        use SketchConstraint as K;
        match *self {
            K::Horizontal(l) | K::Vertical(l) => vec![l],
            K::Diameter { entity, .. } => vec![entity],
            // Every binary constraint, whatever the operands' roles: this
            // reports *which* geometry is named, and a point-on-line names two
            // ids exactly as parallel-lines does.
            K::Coincident(a, b)
            | K::Parallel(a, b)
            | K::Perpendicular(a, b)
            | K::EqualLength(a, b)
            | K::EqualRadius(a, b)
            | K::Distance { a, b, .. }
            | K::Angle { a, b, .. }
            | K::CurveCurveTangent { a, b, .. }
            | K::PointOnLine { point: a, line: b }
            | K::Midpoint { point: a, line: b }
            | K::PointLineDistance {
                point: a, line: b, ..
            }
            | K::PointOnCircle {
                point: a,
                circle: b,
            }
            | K::ArcLineTangent {
                arc: a, line: b, ..
            } => vec![a, b],
            K::SymmetricAboutLine { a, b, line } => vec![a, b, line],
        }
    }
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
            anchor: None,
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

    /// Promote reference geometry back to part of the profile.
    pub fn unmark_construction(&mut self, id: SketchId) {
        self.construction.retain(|c| *c != id);
    }

    /// Delete an entity, and any constraint that named it.
    ///
    /// Leaving a constraint pointing at deleted geometry would make the next
    /// solve a structural error, so removal cascades. Points are deliberately
    /// *not* cascaded: they are shared between entities, and deleting one
    /// segment of a chain must not dissolve its neighbours' endpoints.
    pub fn remove_entity(&mut self, id: SketchId) {
        self.entities.retain(|e| e.id() != id);
        self.construction.retain(|c| *c != id);
        self.constraints.retain(|c| !c.operands().contains(&id));
    }

    /// Delete a point, along with every entity built on it and every
    /// constraint that named either.
    pub fn remove_point(&mut self, id: SketchId) {
        let orphaned: Vec<SketchId> = self
            .entities
            .iter()
            .filter(|e| entity_uses_point(e, id))
            .map(SketchEntity::id)
            .collect();
        for e in orphaned {
            self.remove_entity(e);
        }
        self.points.retain(|p| p.id != id);
        self.constraints.retain(|c| !c.operands().contains(&id));
    }
}

/// Whether `e` is built on point `id`.
fn entity_uses_point(e: &SketchEntity, id: SketchId) -> bool {
    match *e {
        SketchEntity::Line { a, b, .. } => a == id || b == id,
        SketchEntity::Circle { center, .. } => center == id,
        SketchEntity::Arc {
            center, start, end, ..
        } => center == id || start == id || end == id,
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
        doc.unmark_construction(line);
        assert!(
            !doc.is_construction(line),
            "reference geometry promotes back"
        );
    }

    #[test]
    fn removing_an_entity_takes_its_constraints_with_it() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::ZERO);
        let b = doc.add_point(Vec2::X);
        let line = doc.add_line(a, b);
        doc.constrain(SketchConstraint::Horizontal(line));
        doc.mark_construction(line);

        doc.remove_entity(line);

        assert!(doc.entities.is_empty());
        assert!(
            doc.constraints.is_empty(),
            "a constraint naming deleted geometry would be a structural error \
             on the next solve, so removal has to cascade"
        );
        assert!(!doc.is_construction(line));
        assert_eq!(doc.points.len(), 2, "shared points outlive one segment");
    }

    #[test]
    fn removing_a_point_dissolves_what_was_built_on_it() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::ZERO);
        let b = doc.add_point(Vec2::X);
        let c = doc.add_point(Vec2::Y);
        let ab = doc.add_line(a, b);
        let bc = doc.add_line(b, c);
        doc.constrain(SketchConstraint::Perpendicular(ab, bc));

        doc.remove_point(b);

        assert!(doc.points.iter().all(|p| p.id != b));
        assert!(
            doc.entities.is_empty(),
            "both segments were built on the deleted point"
        );
        assert!(doc.constraints.is_empty());
    }

    #[test]
    fn removing_a_point_leaves_unrelated_geometry_alone() {
        let mut doc = SketchDoc::new();
        let a = doc.add_point(Vec2::ZERO);
        let b = doc.add_point(Vec2::X);
        let keep = doc.add_line(a, b);
        let stray = doc.add_point(Vec2::new(9.0, 9.0));

        doc.remove_point(stray);

        assert_eq!(doc.entities.len(), 1);
        assert_eq!(doc.entities[0].id(), keep);
        assert_eq!(doc.points.len(), 2);
    }
}
