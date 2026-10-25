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
//!
//! Tangency lives here too: [`tangent_points`] (the line from a reference
//! point that just grazes a circle) and [`tangent_radius`] (the radius at
//! which a circle about a centre first touches an outline).

use crate::contours::Contours;
use crate::polygonize::polygonize;
use crate::shape::ShapeDef;
use crate::snapping::project_on_segment;
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

/// The two points on the circle (`center`, `radius`) where a line from `q`
/// is tangent to it, or `None` when `q` is inside or on the circle (there
/// is no tangent line from there) or the circle is degenerate.
///
/// Each point `t` satisfies `(t − center) · (t − q) = 0`: the radius to the
/// touch point is perpendicular to the line. They are
/// `c + r·rot(±acos(r/|q−c|))·(q−c)/|q−c|`, counter-clockwise first.
pub fn tangent_points(q: Vec2, center: Vec2, radius: f32) -> Option<[Vec2; 2]> {
    let d = q - center;
    let dist = d.length();
    if !(radius > 0.0 && dist > radius && dist.is_finite()) {
        return None;
    }
    let u = d / dist * radius;
    let half = (radius / dist).acos();
    Some([
        center + Vec2::from_angle(half).rotate(u),
        center + Vec2::from_angle(-half).rotate(u),
    ])
}

/// Where a circle grown about `center` first touches an outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TangentTouch {
    /// The touch point, in the outline's frame.
    pub point: Vec2,
    /// The circle's radius at first contact: `center.distance(point)`.
    pub radius: f32,
}

/// The tangent radius from `center` to `outline`: the nearest outline point
/// and its distance. A circle of that radius about `center` just touches
/// the outline — externally when `center` is outside the shape (for a
/// circle (c₂, r₂): `|c₂ − c| − r₂`), internally when inside
/// (`r₂ − |c₂ − c|`).
///
/// `None` for an empty outline, when `center` lies on the outline (the
/// radius would be zero), or at a circle's own centre (every point is
/// equally near, so there is no single touch point).
pub fn tangent_radius(center: Vec2, outline: &SnapOutline) -> Option<TangentTouch> {
    let point = match outline {
        SnapOutline::Circle {
            center: c2,
            radius: r2,
            ..
        } => {
            let dir = (center - *c2).try_normalize()?;
            *c2 + dir * *r2
        }
        SnapOutline::Contours(contours) => contours
            .rings()
            .flat_map(|ring| {
                let n = ring.len();
                (0..n).map(move |i| project_on_segment(center, ring[i], ring[(i + 1) % n]))
            })
            .min_by(|a, b| {
                center
                    .distance_squared(*a)
                    .total_cmp(&center.distance_squared(*b))
            })?,
    };
    let radius = center.distance(point);
    (radius > f32::EPSILON).then_some(TangentTouch { point, radius })
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

    #[test]
    fn tangent_points_lie_on_the_circle_perpendicular_to_the_radius() {
        let c = Vec2::new(2.0, -1.0);
        let r = 3.0;
        for q in [
            Vec2::new(10.0, 0.0),
            Vec2::new(-4.0, 7.0),
            Vec2::new(2.0, -4.5),
        ] {
            let [t0, t1] = tangent_points(q, c, r).unwrap();
            assert!(!close(t0, t1), "two distinct points from {q}");
            for t in [t0, t1] {
                assert!((t.distance(c) - r).abs() < 1e-4, "{t} not on the circle");
                let dot = (t - c).normalize().dot((t - q).normalize());
                assert!(dot.abs() < 1e-4, "radius not perpendicular at {t}: {dot}");
            }
        }
    }

    #[test]
    fn tangent_points_from_a_known_point() {
        // From (2, 0) to the unit circle: acos(1/2) = 60°.
        let [a, b] = tangent_points(Vec2::new(2.0, 0.0), Vec2::ZERO, 1.0).unwrap();
        let h = 3f32.sqrt() / 2.0;
        assert!(close(a, Vec2::new(0.5, h)), "{a}");
        assert!(close(b, Vec2::new(0.5, -h)), "{b}");
    }

    #[test]
    fn no_tangent_points_from_inside_or_on_the_circle() {
        let c = Vec2::new(1.0, 1.0);
        assert!(tangent_points(c, c, 2.0).is_none());
        assert!(tangent_points(Vec2::new(2.0, 1.5), c, 2.0).is_none());
        assert!(
            tangent_points(Vec2::new(3.0, 1.0), c, 2.0).is_none(),
            "on it"
        );
        assert!(
            tangent_points(Vec2::new(9.0, 1.0), c, 0.0).is_none(),
            "degenerate"
        );
    }

    #[test]
    fn tangent_radius_to_a_circle_outside_and_inside() {
        let outline = snap_outline(&ShapeDef::Circle { radius: 2.0 });
        // Outside: |c₂ − c| − r₂.
        let t = tangent_radius(Vec2::new(5.0, 0.0), &outline).unwrap();
        assert!((t.radius - 3.0).abs() < 1e-5);
        assert!(close(t.point, Vec2::new(2.0, 0.0)));
        // Inside: r₂ − |c₂ − c|.
        let t = tangent_radius(Vec2::new(0.0, 0.5), &outline).unwrap();
        assert!((t.radius - 1.5).abs() < 1e-5);
        assert!(close(t.point, Vec2::new(0.0, 2.0)));
        // Concentric or on the outline: no single touch.
        assert!(tangent_radius(Vec2::ZERO, &outline).is_none());
        assert!(tangent_radius(Vec2::new(0.0, 2.0), &outline).is_none());
    }

    #[test]
    fn tangent_radius_to_a_box_is_the_nearest_side_or_corner() {
        let outline = snap_outline(&ShapeDef::Box {
            width: 4.0,
            height: 2.0,
        });
        // Facing the right side: touches its middle.
        let t = tangent_radius(Vec2::new(5.0, 0.0), &outline).unwrap();
        assert!((t.radius - 3.0).abs() < 1e-4, "{}", t.radius);
        assert!(close(t.point, Vec2::new(2.0, 0.0)), "{}", t.point);
        // Diagonally off a corner: touches the corner.
        let t = tangent_radius(Vec2::new(5.0, 5.0), &outline).unwrap();
        assert!(close(t.point, Vec2::new(2.0, 1.0)), "{}", t.point);
        assert!((t.radius - 5.0).abs() < 1e-4, "{}", t.radius);
        // From inside: the nearest side, internally.
        let t = tangent_radius(Vec2::new(0.0, 0.25), &outline).unwrap();
        assert!((t.radius - 0.75).abs() < 1e-4, "{}", t.radius);
    }
}
