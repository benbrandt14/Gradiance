//! Pure scaling math: shapes under a linear map, exactly.
//!
//! Scaling happens in a *frame* (world axes or a body-local frame). For a
//! body rotated by `body_rot`, scaling by `factors` along the axes of a
//! frame rotated by `frame_rot` transforms local geometry by
//! `M = R(-body_rot) · R(frame_rot) · S · R(-frame_rot) · R(body_rot)`.
//!
//! Boxes and circles stay parametric when `M` keeps them representable
//! (axis-aligned diagonal / uniform); otherwise they **polygonize exactly**
//! (the same convention as CSG) instead of being silently approximated.

use crate::domain::shape::ShapeDef;
use crate::geometry::polygonize::polygonize;
use bevy::math::{Mat2, Vec2};

/// Numeric tolerance for "no shear / uniform" checks.
const EPS: f32 = 1e-4;

/// The body-local linear map for scaling by `factors` along `frame_rot`
/// axes, applied to a body rotated by `body_rot`.
pub fn body_scale_matrix(body_rot: f32, frame_rot: f32, factors: Vec2) -> Mat2 {
    let rb = Mat2::from_angle(body_rot);
    let rf = Mat2::from_angle(frame_rot);
    rb.inverse() * rf * Mat2::from_diagonal(factors) * rf.inverse() * rb
}

/// Maps a world-space point through the frame scale about `pivot`.
pub fn scale_point(p: Vec2, pivot: Vec2, frame_rot: f32, factors: Vec2) -> Vec2 {
    let rf = Mat2::from_angle(frame_rot);
    pivot + rf * (factors * (rf.inverse() * (p - pivot)))
}

fn is_diagonal(m: Mat2) -> bool {
    m.x_axis.y.abs() < EPS && m.y_axis.x.abs() < EPS
}

fn is_uniform(m: Mat2) -> bool {
    is_diagonal(m) && (m.x_axis.x.abs() - m.y_axis.y.abs()).abs() < EPS
}

fn map_ring(ring: &[Vec2], m: Mat2) -> Vec<Vec2> {
    ring.iter().map(|v| m * *v).collect()
}

/// Applies a body-local linear map to a shape, exactly.
///
/// Falls back from parametric shapes to polygons when the map is not
/// representable (shear on a box, non-uniform on a circle). If the map
/// flips orientation, ring windings are reversed to stay CCW/CW-correct.
/// `HalfPlane` is scale-invariant.
pub fn scale_shape(shape: &ShapeDef, m: Mat2) -> ShapeDef {
    let flips = m.determinant() < 0.0;
    let mapped_polygon = |shape: &ShapeDef| {
        let contours = polygonize(shape);
        let mut outline = map_ring(&contours.outline, m);
        let mut holes: Vec<Vec<Vec2>> = contours.holes.iter().map(|h| map_ring(h, m)).collect();
        if flips {
            outline.reverse();
            for h in &mut holes {
                h.reverse();
            }
        }
        ShapeDef::Polygon { outline, holes }
    };

    match shape {
        ShapeDef::Box { width, height } if is_diagonal(m) => ShapeDef::Box {
            width: width * m.x_axis.x.abs(),
            height: height * m.y_axis.y.abs(),
        },
        ShapeDef::Circle { radius } if is_uniform(m) => ShapeDef::Circle {
            radius: radius * m.x_axis.x.abs(),
        },
        ShapeDef::Capsule {
            half_length,
            radius,
        } if is_uniform(m) => ShapeDef::Capsule {
            half_length: half_length * m.x_axis.x.abs(),
            radius: radius * m.x_axis.x.abs(),
        },
        ShapeDef::HalfPlane => ShapeDef::HalfPlane,
        other => mapped_polygon(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::contours::Contours;

    #[test]
    fn aligned_box_scaling_stays_parametric() {
        let m = body_scale_matrix(0.0, 0.0, Vec2::new(2.0, 0.5));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        assert_eq!(
            scaled,
            ShapeDef::Box {
                width: 80.0,
                height: 10.0
            }
        );
    }

    #[test]
    fn quarter_turned_box_swaps_axes_and_stays_parametric() {
        let m = body_scale_matrix(std::f32::consts::FRAC_PI_2, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        // World-X scaling of a 90°-rotated box stretches its local Y.
        let ShapeDef::Box { width, height } = scaled else {
            panic!("expected parametric box, got {scaled:?}");
        };
        assert!((width - 40.0).abs() < 1e-3);
        assert!((height - 40.0).abs() < 1e-3);
    }

    #[test]
    fn off_axis_box_polygonizes_with_correct_area() {
        let rot = 30_f32.to_radians();
        let m = body_scale_matrix(rot, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(
            &ShapeDef::Box {
                width: 40.0,
                height: 20.0,
            },
            m,
        );
        let ShapeDef::Polygon { outline, holes } = &scaled else {
            panic!("expected polygonized result, got {scaled:?}");
        };
        let area = Contours {
            outline: outline.clone(),
            holes: holes.clone(),
        }
        .area();
        assert!((area - 1600.0).abs() < 1.0, "area doubles (got {area})");
    }

    #[test]
    fn circles_stay_circles_only_under_uniform_scale() {
        let uniform = body_scale_matrix(0.3, 1.1, Vec2::splat(3.0));
        let ShapeDef::Circle { radius } = scale_shape(&ShapeDef::Circle { radius: 10.0 }, uniform)
        else {
            panic!("uniform scale must keep circles parametric");
        };
        assert!((radius - 30.0).abs() < 1e-3, "radius {radius}");

        let squash = body_scale_matrix(0.0, 0.0, Vec2::new(2.0, 1.0));
        let scaled = scale_shape(&ShapeDef::Circle { radius: 10.0 }, squash);
        let ShapeDef::Polygon { outline, .. } = &scaled else {
            panic!("expected ellipse polygon, got {scaled:?}");
        };
        let area = crate::geometry::contours::ring_signed_area(outline).abs();
        let ideal = std::f32::consts::PI * 10.0 * 10.0 * 2.0;
        assert!((area - ideal).abs() / ideal < 0.02, "{area} vs {ideal}");
    }

    #[test]
    fn scale_point_moves_positions_about_the_pivot() {
        let p = scale_point(Vec2::new(10.0, 0.0), Vec2::ZERO, 0.0, Vec2::new(2.0, 1.0));
        assert_eq!(p, Vec2::new(20.0, 0.0));
        // Pivot is a fixed point.
        let pivot = Vec2::new(5.0, 5.0);
        assert_eq!(scale_point(pivot, pivot, 0.7, Vec2::new(3.0, 0.2)), pivot);
    }

    #[test]
    fn mirroring_keeps_winding_valid() {
        let m = body_scale_matrix(0.0, 0.0, Vec2::new(-1.0, 1.0));
        let tri = ShapeDef::Polygon {
            outline: vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(0.0, 10.0),
            ],
            holes: vec![],
        };
        let ShapeDef::Polygon { outline, .. } = scale_shape(&tri, m) else {
            panic!()
        };
        assert!(
            crate::geometry::contours::ring_signed_area(&outline) > 0.0,
            "outline stays CCW after mirror"
        );
    }
}
