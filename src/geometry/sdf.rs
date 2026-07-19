//! Signed-distance evaluation of [`ShapeDef`] trees.
//!
//! Every body's geometry is an SDF: negative inside, positive outside,
//! zero on the boundary. Leaves ([`Box`](ShapeDef::Box),
//! [`Circle`](ShapeDef::Circle), …) are exact distances;
//! [`Placed`](ShapeDef::Placed) is a rigid isometry so exactness is
//! preserved. After a CSG boolean the field is a *conservative bound*
//! rather than a true distance (the classic min/max property) — still
//! exact on the boundary, which is all contouring
//! ([`geometry::contour`](crate::geometry::contour)) and field forces
//! need.
//!
//! This is the seam that makes CSG cheap: subtracting a shape is one
//! [`Subtract`](CsgOp::Subtract) node, merging two bodies is one
//! [`Union`](CsgOp::Union) node — no mesh booleans.
//!
//! ```
//! use gradiance::domain::shape::{CsgOp, ShapeDef};
//! use gradiance::geometry::sdf::eval;
//! use bevy::math::Vec2;
//!
//! // A disc of radius 10: the field is exactly `|p| - r`.
//! let disc = ShapeDef::Circle { radius: 10.0 };
//! assert!((eval(&disc, Vec2::ZERO) + 10.0).abs() < 1e-5); // -10 at center
//! assert!((eval(&disc, Vec2::new(10.0, 0.0))).abs() < 1e-5); // 0 on rim
//!
//! // Punch a hole: disc − a small disc at the origin. The center that
//! // was solid (-10) is now outside the material (positive).
//! let ring = ShapeDef::Csg {
//!     op: CsgOp::Subtract,
//!     lhs: Box::new(disc),
//!     rhs: Box::new(ShapeDef::Circle { radius: 4.0 }),
//! };
//! assert!(eval(&ring, Vec2::ZERO) > 0.0);
//! assert!(eval(&ring, Vec2::new(7.0, 0.0)) < 0.0); // still solid mid-ring
//! ```

use crate::core::constants::{GROUND_SLAB_DEPTH, GROUND_SLAB_WIDTH};
use crate::domain::shape::{CsgOp, ShapeDef};
use bevy::math::Vec2;

/// The field gradient of `shape`'s SDF at `p` (central differences).
/// Points away from the surface outside the shape; unit-ish length near
/// smooth boundaries. Used by field forces (magnetism) to aim along the
/// actual surface normal instead of at the body center.
///
/// ```
/// # use gradiance::geometry::sdf::gradient;
/// # use gradiance::domain::shape::ShapeDef;
/// # use bevy::math::Vec2;
/// let circle = ShapeDef::Circle { radius: 10.0 };
/// let g = gradient(&circle, Vec2::new(20.0, 0.0), 0.1);
/// assert!((g - Vec2::X).length() < 1e-2);
/// ```
pub fn gradient(shape: &ShapeDef, p: Vec2, eps: f32) -> Vec2 {
    let dx = eval(shape, p + Vec2::X * eps) - eval(shape, p - Vec2::X * eps);
    let dy = eval(shape, p + Vec2::Y * eps) - eval(shape, p - Vec2::Y * eps);
    Vec2::new(dx, dy) / (2.0 * eps)
}

/// Evaluates the signed distance field at `p` (shape-local space).
pub fn eval(shape: &ShapeDef, p: Vec2) -> f32 {
    match shape {
        ShapeDef::Box { width, height } => {
            let half = Vec2::new(width / 2.0, height / 2.0);
            let q = p.abs() - half;
            q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
        }
        ShapeDef::Circle { radius } => p.length() - radius,
        ShapeDef::Polygon { outline, holes } => {
            let rings = std::iter::once(outline).chain(holes.iter());
            let mut dist = f32::MAX;
            let mut inside = false;
            for ring in rings {
                dist = dist.min(ring_distance(p, ring));
                if crate::geometry::contours::point_in_ring(p, ring) {
                    inside = !inside;
                }
            }
            if inside { -dist } else { dist }
        }
        ShapeDef::Capsule {
            half_length,
            radius,
        } => {
            // Exact distance to the x-axis segment, inflated by the radius.
            let x = p.x.clamp(-half_length, *half_length);
            (p - Vec2::new(x, 0.0)).length() - radius
        }
        // Solid below the local y = 0 surface.
        ShapeDef::HalfPlane => p.y,
        ShapeDef::Csg { op, lhs, rhs } => {
            let a = eval(lhs, p);
            let b = eval(rhs, p);
            match op {
                CsgOp::Union => a.min(b),
                CsgOp::Subtract => a.max(-b),
                CsgOp::Intersect => a.max(b),
                CsgOp::SmoothUnion { radius } => {
                    // Polynomial smooth-min (quadratic), C1 everywhere.
                    let h = (0.5 + 0.5 * (b - a) / radius).clamp(0.0, 1.0);
                    b + (a - b) * h - radius * h * (1.0 - h)
                }
            }
        }
        ShapeDef::Placed { pos, rot, shape } => eval(shape, Vec2::from_angle(-rot).rotate(p - pos)),
    }
}

/// Unsigned distance from `p` to the closed ring boundary.
fn ring_distance(p: Vec2, ring: &[Vec2]) -> f32 {
    let n = ring.len();
    if n == 0 {
        return f32::MAX;
    }
    let mut best = f32::MAX;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let ab = b - a;
        let t = if ab.length_squared() > 0.0 {
            ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        best = best.min(p.distance(a + ab * t));
    }
    best
}

/// A conservative axis-aligned bounding box `(min, max)` of the solid
/// region, in shape-local space.
///
/// Boolean bounds compose conservatively (`Subtract` keeps the left
/// operand's box; `SmoothUnion` inflates by the blend radius). The
/// infinite half-plane reports its rendered slab.
pub fn aabb(shape: &ShapeDef) -> (Vec2, Vec2) {
    match shape {
        ShapeDef::Box { width, height } => {
            let half = Vec2::new(width / 2.0, height / 2.0);
            (-half, half)
        }
        ShapeDef::Circle { radius } => (Vec2::splat(-radius), Vec2::splat(*radius)),
        ShapeDef::Polygon { outline, holes } => {
            let mut min = Vec2::splat(f32::MAX);
            let mut max = Vec2::splat(f32::MIN);
            for v in outline.iter().chain(holes.iter().flatten()) {
                min = min.min(*v);
                max = max.max(*v);
            }
            if min.x > max.x {
                (Vec2::ZERO, Vec2::ZERO)
            } else {
                (min, max)
            }
        }
        ShapeDef::Capsule {
            half_length,
            radius,
        } => (
            Vec2::new(-half_length - radius, -radius),
            Vec2::new(half_length + radius, *radius),
        ),
        ShapeDef::HalfPlane => (
            Vec2::new(-GROUND_SLAB_WIDTH / 2.0, -GROUND_SLAB_DEPTH),
            Vec2::new(GROUND_SLAB_WIDTH / 2.0, 0.0),
        ),
        ShapeDef::Csg { op, lhs, rhs } => {
            let (amin, amax) = aabb(lhs);
            match op {
                CsgOp::Subtract => (amin, amax),
                CsgOp::Union => {
                    let (bmin, bmax) = aabb(rhs);
                    (amin.min(bmin), amax.max(bmax))
                }
                CsgOp::Intersect => {
                    let (bmin, bmax) = aabb(rhs);
                    let min = amin.max(bmin);
                    let max = amax.min(bmax);
                    if min.x > max.x || min.y > max.y {
                        (Vec2::ZERO, Vec2::ZERO)
                    } else {
                        (min, max)
                    }
                }
                CsgOp::SmoothUnion { radius } => {
                    let (bmin, bmax) = aabb(rhs);
                    let pad = Vec2::splat(*radius);
                    (amin.min(bmin) - pad, amax.max(bmax) + pad)
                }
            }
        }
        ShapeDef::Placed { pos, rot, shape } => {
            let (min, max) = aabb(shape);
            let r = Vec2::from_angle(*rot);
            let corners = [
                Vec2::new(min.x, min.y),
                Vec2::new(max.x, min.y),
                Vec2::new(max.x, max.y),
                Vec2::new(min.x, max.y),
            ];
            let mut out_min = Vec2::splat(f32::MAX);
            let mut out_max = Vec2::splat(f32::MIN);
            for c in corners {
                let w = *pos + r.rotate(c);
                out_min = out_min.min(w);
                out_max = out_max.max(w);
            }
            (out_min, out_max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_fields_are_exact_distances() {
        let b = ShapeDef::Box {
            width: 20.0,
            height: 10.0,
        };
        assert!((eval(&b, Vec2::ZERO) + 5.0).abs() < 1e-5, "deep inside");
        assert!((eval(&b, Vec2::new(15.0, 0.0)) - 5.0).abs() < 1e-5);
        assert!(
            eval(&b, Vec2::new(10.0, 5.0)).abs() < 1e-5,
            "corner on boundary"
        );

        let c = ShapeDef::Circle { radius: 10.0 };
        assert!((eval(&c, Vec2::new(3.0, 4.0)) + 5.0).abs() < 1e-5);
        assert!((eval(&c, Vec2::new(30.0, 40.0)) - 40.0).abs() < 1e-4);
    }

    #[test]
    fn capsule_field_is_exact_at_caps_sides_and_inside() {
        let rod = ShapeDef::Capsule {
            half_length: 20.0,
            radius: 2.5,
        };
        // Side: straight above the segment midpoint.
        assert!((eval(&rod, Vec2::new(0.0, 5.0)) - 2.5).abs() < 1e-5);
        // Cap: beyond the right endpoint along the axis.
        assert!((eval(&rod, Vec2::new(30.0, 0.0)) - 7.5).abs() < 1e-5);
        // Cap surface point at 45° off the left endpoint (outward, past
        // the segment end, where the cap arc is the nearest surface).
        let offset = 2.5 / std::f32::consts::SQRT_2;
        let p = Vec2::new(-20.0 - offset, offset);
        assert!(eval(&rod, p).abs() < 1e-5);
        // Deep inside: on the segment itself.
        assert!((eval(&rod, Vec2::new(10.0, 0.0)) + 2.5).abs() < 1e-5);
    }

    #[test]
    fn polygon_field_signs_respect_holes() {
        let square = vec![
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(-10.0, 10.0),
        ];
        let mut hole = vec![
            Vec2::new(-5.0, -5.0),
            Vec2::new(5.0, -5.0),
            Vec2::new(5.0, 5.0),
            Vec2::new(-5.0, 5.0),
        ];
        hole.reverse();
        let p = ShapeDef::Polygon {
            outline: square,
            holes: vec![hole],
        };
        assert!(eval(&p, Vec2::new(7.5, 0.0)) < 0.0, "solid band");
        assert!(eval(&p, Vec2::ZERO) > 0.0, "hole interior is outside");
        assert!(eval(&p, Vec2::new(20.0, 0.0)) > 0.0);
    }

    #[test]
    fn subtract_and_placed_compose() {
        // A 40×40 box minus a circle of r=10 placed at its center-right edge.
        let shape = ShapeDef::Csg {
            op: CsgOp::Subtract,
            lhs: Box::new(ShapeDef::Box {
                width: 40.0,
                height: 40.0,
            }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(20.0, 0.0),
                rot: 0.0,
                shape: Box::new(ShapeDef::Circle { radius: 10.0 }),
            }),
        };
        assert!(eval(&shape, Vec2::new(15.0, 0.0)) > 0.0, "inside the bite");
        assert!(eval(&shape, Vec2::new(0.0, 0.0)) < 0.0, "body interior");
        assert!(eval(&shape, Vec2::new(-19.0, 0.0)) < 0.0);
    }

    #[test]
    fn placed_rotation_is_an_isometry() {
        let rotated = ShapeDef::Placed {
            pos: Vec2::ZERO,
            rot: std::f32::consts::FRAC_PI_4,
            shape: Box::new(ShapeDef::Box {
                width: 20.0,
                height: 20.0,
            }),
        };
        // The rotated box's corner lies on the +x axis at 10·√2.
        let corner = 10.0 * std::f32::consts::SQRT_2;
        assert!(eval(&rotated, Vec2::new(corner, 0.0)).abs() < 1e-4);
        assert!(eval(&rotated, Vec2::new(corner + 5.0, 0.0)) > 0.0);
    }

    #[test]
    fn aabbs_bound_their_solids() {
        let shape = ShapeDef::Csg {
            op: CsgOp::Union,
            lhs: Box::new(ShapeDef::Circle { radius: 5.0 }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(30.0, 0.0),
                rot: 1.0,
                shape: Box::new(ShapeDef::Box {
                    width: 10.0,
                    height: 4.0,
                }),
            }),
        };
        let (min, max) = aabb(&shape);
        // Sample a coarse grid; every inside point must be inside the box.
        for ix in -50..=50 {
            for iy in -50..=50 {
                let p = Vec2::new(ix as f32, iy as f32);
                if eval(&shape, p) < 0.0 {
                    assert!(
                        p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y,
                        "{p} escapes aabb {min}..{max}"
                    );
                }
            }
        }
    }
}
