//! Snap features of a shape: what an object snapper may offer as targets.
//!
//! A polygonal outline's vertices and edge midpoints are real features of
//! the authored geometry. A circle's are not: [`polygonize`] turns it into
//! [`CIRCLE_SEGMENTS`](gradiance_core::constants::CIRCLE_SEGMENTS)
//! tessellation vertices a few pixels apart, and offering those (and their
//! midpoints) makes the snap kind alternate vertex / edge / midpoint as the
//! cursor slides along the outline. So analytic curved leaves are queried
//! **directly** — the leaf is the source of truth — and only shapes with no
//! analytic curve (boxes, polygons, CSG trees) take the polygonized path.
//! Reading a leaf's own centre and radius is not contouring a field, so
//! [`polygonize`] stays the single discretization point.

use crate::contours::Contours;
use crate::polygonize::polygonize;
use crate::shape::ShapeDef;
use bevy::math::Vec2;

/// The outline an object snapper queries for one shape, in the shape's
/// local frame.
#[derive(Debug, Clone, PartialEq)]
pub enum SnapOutline {
    /// An exact circle. Its snap features are the centre, the four
    /// [`quadrants`](SnapOutline::circle_quadrants) (as vertices), and the
    /// exact [nearest point](nearest_on_circle) on the circle (as the edge)
    /// — no midpoints, since a circle has no straight edges.
    Circle {
        /// Centre in the shape's local frame.
        center: Vec2,
        /// Radius, same units as the shape.
        radius: f32,
        /// Rotation of the quadrant points relative to the shape's local
        /// axes, radians (non-zero only for a rotated [`ShapeDef::Placed`]).
        rotation: f32,
    },
    /// A polygonal outline: features are ring vertices, edge midpoints,
    /// and the nearest point on each edge.
    Contours(Contours),
}

impl SnapOutline {
    /// The four quadrant points of a [`SnapOutline::Circle`] — at 0°, 90°,
    /// 180° and 270° of its rotated frame — or `None` for a polygonal
    /// outline.
    pub fn circle_quadrants(&self) -> Option<[Vec2; 4]> {
        match self {
            Self::Circle {
                center,
                radius,
                rotation,
            } => {
                let u = Vec2::from_angle(*rotation) * *radius;
                Some([
                    *center + u,
                    *center + u.perp(),
                    *center - u,
                    *center - u.perp(),
                ])
            }
            Self::Contours(_) => None,
        }
    }
}

/// The outline a snapper should query for `shape`: an exact circle for a
/// [`ShapeDef::Circle`] (bare, or rigidly [`Placed`](ShapeDef::Placed) at
/// any nesting depth), otherwise the polygonized contours.
pub fn snap_outline(shape: &ShapeDef) -> SnapOutline {
    match placed_circle(shape) {
        Some((center, radius, rotation)) => SnapOutline::Circle {
            center,
            radius,
            rotation,
        },
        None => SnapOutline::Contours(polygonize(shape)),
    }
}

/// `(centre, radius, rotation)` when `shape` is a circle under zero or more
/// rigid placements.
fn placed_circle(shape: &ShapeDef) -> Option<(Vec2, f32, f32)> {
    match shape {
        ShapeDef::Circle { radius } => Some((Vec2::ZERO, *radius, 0.0)),
        ShapeDef::Placed { pos, rot, shape } => {
            let (c, r, inner) = placed_circle(shape)?;
            Some((*pos + Vec2::from_angle(*rot).rotate(c), r, rot + inner))
        }
        _ => None,
    }
}

/// The point on the circle (`center`, `radius`) nearest to `p`.
///
/// At the centre itself every point is equally near; the result is then
/// the 0° point, so the answer is total and deterministic.
pub fn nearest_on_circle(p: Vec2, center: Vec2, radius: f32) -> Vec2 {
    center + (p - center).try_normalize().unwrap_or(Vec2::X) * radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shape::CsgOp;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn a_bare_circle_snaps_as_an_exact_circle() {
        let outline = snap_outline(&ShapeDef::Circle { radius: 50.0 });
        assert_eq!(
            outline,
            SnapOutline::Circle {
                center: Vec2::ZERO,
                radius: 50.0,
                rotation: 0.0
            }
        );
        let q = outline.circle_quadrants().unwrap();
        for (got, want) in q.iter().zip([
            Vec2::new(50.0, 0.0),
            Vec2::new(0.0, 50.0),
            Vec2::new(-50.0, 0.0),
            Vec2::new(0.0, -50.0),
        ]) {
            assert!(close(*got, want), "{got} != {want}");
        }
    }

    #[test]
    fn a_placed_circle_composes_its_placements() {
        // Outer placement rotates 90° and moves; inner moves 10 along its x.
        let shape = ShapeDef::Placed {
            pos: Vec2::new(5.0, 0.0),
            rot: std::f32::consts::FRAC_PI_2,
            shape: Box::new(ShapeDef::Placed {
                pos: Vec2::new(10.0, 0.0),
                rot: 0.0,
                shape: Box::new(ShapeDef::Circle { radius: 2.0 }),
            }),
        };
        let SnapOutline::Circle {
            center,
            radius,
            rotation,
        } = snap_outline(&shape)
        else {
            panic!("expected an exact circle");
        };
        assert!(close(center, Vec2::new(5.0, 10.0)), "{center}");
        assert!((radius - 2.0).abs() < 1e-6);
        assert!((rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // The first quadrant follows the rotation: +x of the placed frame
        // is world +y.
        let q = snap_outline(&shape).circle_quadrants().unwrap();
        assert!(close(q[0], Vec2::new(5.0, 12.0)), "{}", q[0]);
    }

    #[test]
    fn non_circles_take_the_polygonized_path() {
        let boxed = ShapeDef::Box {
            width: 4.0,
            height: 2.0,
        };
        assert_eq!(
            snap_outline(&boxed),
            SnapOutline::Contours(polygonize(&boxed))
        );
        assert!(snap_outline(&boxed).circle_quadrants().is_none());
        // A CSG tree is contoured even when an operand is a circle.
        let csg = ShapeDef::Csg {
            op: CsgOp::Union,
            lhs: Box::new(ShapeDef::Circle { radius: 1.0 }),
            rhs: Box::new(boxed),
        };
        assert!(matches!(snap_outline(&csg), SnapOutline::Contours(_)));
    }

    #[test]
    fn nearest_on_circle_projects_radially_and_is_total_at_the_centre() {
        let c = Vec2::new(1.0, 1.0);
        assert!(close(
            nearest_on_circle(Vec2::new(4.0, 5.0), c, 10.0),
            c + Vec2::new(6.0, 8.0)
        ));
        // From inside, too.
        assert!(close(
            nearest_on_circle(Vec2::new(1.0, 2.0), c, 10.0),
            Vec2::new(1.0, 11.0)
        ));
        assert!(close(nearest_on_circle(c, c, 10.0), Vec2::new(11.0, 1.0)));
    }
}
