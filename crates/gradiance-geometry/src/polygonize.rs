//! Converting authored shapes into [`Contours`] — the **single
//! discretization point** between SDF trees and every polygon consumer
//! (colliders, meshes, snapping, scaling).

use crate::contour::{FIELD_CELLS, contour_components};
use crate::contours::Contours;
use crate::sdf;
use crate::shape::ShapeDef;
use bevy::math::Vec2;
use gradiance_core::constants::{CIRCLE_SEGMENTS, GROUND_SLAB_DEPTH, GROUND_SLAB_WIDTH};

/// Converts a shape into explicit contours (counter-clockwise outline).
///
/// Leaves take exact paths: circles are discretized with
/// [`CIRCLE_SEGMENTS`] segments; the infinite half-plane becomes a large
/// finite slab (its rendered / CSG stand-in): surface along local
/// `y = 0`, extending [`GROUND_SLAB_DEPTH`] downward. CSG trees are
/// contoured from their distance field; when the field splits into
/// several components this returns the **largest** (commands keep bodies
/// single-component by splitting cut results — see
/// [`polygonize_components`] for all of them).
pub fn polygonize(shape: &ShapeDef) -> Contours {
    match shape {
        ShapeDef::Box { width, height } => rectangle(*width, *height, Vec2::ZERO),
        ShapeDef::Circle { radius } => {
            let outline = (0..CIRCLE_SEGMENTS)
                .map(|i| {
                    let theta = std::f32::consts::TAU * (i as f32) / (CIRCLE_SEGMENTS as f32);
                    Vec2::new(theta.cos(), theta.sin()) * *radius
                })
                .collect();
            Contours {
                outline,
                holes: vec![],
            }
        }
        ShapeDef::Polygon { outline, holes } => Contours {
            outline: outline.clone(),
            holes: holes.clone(),
        },
        ShapeDef::Capsule {
            half_length,
            radius,
        } => Contours {
            outline: stadium(*half_length, *radius),
            holes: vec![],
        },
        ShapeDef::HalfPlane => rectangle(
            GROUND_SLAB_WIDTH,
            GROUND_SLAB_DEPTH,
            Vec2::new(0.0, -GROUND_SLAB_DEPTH / 2.0),
        ),
        ShapeDef::Csg { .. } | ShapeDef::Placed { .. } => polygonize_components(shape)
            .into_iter()
            .max_by(|a, b| {
                a.area()
                    .partial_cmp(&b.area())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(Contours {
                outline: vec![],
                holes: vec![],
            }),
    }
}

/// Converts a shape into contours, one [`Contours`] per connected
/// component (leaves are always a single component).
pub fn polygonize_components(shape: &ShapeDef) -> Vec<Contours> {
    match shape {
        ShapeDef::Csg { .. } | ShapeDef::Placed { .. } => {
            let (min, max) = sdf::aabb(shape);
            let cell = (max - min).max_element() / FIELD_CELLS;
            contour_components(|p| sdf::eval(shape, p), min, max, cell)
        }
        leaf => vec![polygonize(leaf)],
    }
}

/// The stadium outline of a capsule: a semicircle fan around each segment
/// endpoint, counter-clockwise. Cap resolution reuses [`CIRCLE_SEGMENTS`]
/// (half per cap) so rods discretize as finely as circles.
fn stadium(half_length: f32, radius: f32) -> Vec<Vec2> {
    let per_cap = CIRCLE_SEGMENTS / 2;
    let step = std::f32::consts::PI / (per_cap as f32);
    let cap = |center: Vec2, start: f32| {
        (0..=per_cap).map(move |i| {
            let theta = start + step * (i as f32);
            center + Vec2::new(theta.cos(), theta.sin()) * radius
        })
    };
    // Right cap -90 deg to +90 deg, then left cap +90 deg to +270 deg.
    cap(Vec2::new(half_length, 0.0), -std::f32::consts::FRAC_PI_2)
        .chain(cap(
            Vec2::new(-half_length, 0.0),
            std::f32::consts::FRAC_PI_2,
        ))
        .collect()
}

fn rectangle(width: f32, height: f32, center: Vec2) -> Contours {
    let (hw, hh) = (width / 2.0, height / 2.0);
    Contours {
        outline: vec![
            center + Vec2::new(-hw, -hh),
            center + Vec2::new(hw, -hh),
            center + Vec2::new(hw, hh),
            center + Vec2::new(-hw, hh),
        ],
        holes: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contours::ring_signed_area;

    #[test]
    fn capsule_stadium_is_ccw_closed_and_spans_the_rod() {
        let (hl, r) = (0.20, 0.025);
        let rod = ShapeDef::Capsule {
            half_length: hl,
            radius: r,
        };
        let c = polygonize(&rod);
        assert!(c.outline.len() >= CIRCLE_SEGMENTS, "cap resolution");
        assert!(ring_signed_area(&c.outline) > 0.0, "counter-clockwise");
        // Extreme points reach the inflated endpoints.
        let max_x = c.outline.iter().map(|v| v.x).fold(f32::MIN, f32::max);
        let min_x = c.outline.iter().map(|v| v.x).fold(f32::MAX, f32::min);
        assert!((max_x - (hl + r)).abs() < 1e-6);
        assert!((min_x + (hl + r)).abs() < 1e-6);
        // Area ~ rectangle + full disc (inscribed caps lose a little).
        let ideal = 2.0 * hl * 2.0 * r + std::f32::consts::PI * r * r;
        assert!((c.area() - ideal).abs() / ideal < 0.01);
        // Every vertex lies on the SDF zero level.
        for v in &c.outline {
            assert!(sdf::eval(&rod, *v).abs() < 1e-6);
        }
    }

    #[test]
    fn box_becomes_ccw_rectangle_of_right_area() {
        let c = polygonize(&ShapeDef::Box {
            width: 40.0,
            height: 20.0,
        });
        assert_eq!(c.outline.len(), 4);
        assert!((ring_signed_area(&c.outline) - 800.0).abs() < 1e-3);
    }

    #[test]
    fn circle_discretization_hits_area_within_tolerance() {
        let r = 50.0;
        let c = polygonize(&ShapeDef::Circle { radius: r });
        assert_eq!(c.outline.len(), CIRCLE_SEGMENTS);
        let ideal = std::f32::consts::PI * r * r;
        let got = c.area();
        assert!(
            (got - ideal).abs() / ideal < 0.01,
            "48-gon area within 1% of circle ({got} vs {ideal})"
        );
    }

    #[test]
    fn half_plane_slab_sits_below_its_surface_line() {
        let c = polygonize(&ShapeDef::HalfPlane);
        assert!(c.outline.iter().all(|v| v.y <= 0.0));
        assert!(ring_signed_area(&c.outline) > 0.0);
    }

    /// Two boxes in an L (a 2 x 0.5 bar and a 0.5 x 1.5 post on its left
    /// end), joined by a smooth union of `radius`; the concave corner is
    /// at `(-0.5, 0.25)`.
    fn filleted_l(radius: f32) -> ShapeDef {
        ShapeDef::Csg {
            op: crate::shape::CsgOp::SmoothUnion { radius },
            lhs: Box::new(ShapeDef::Box {
                width: 2.0,
                height: 0.5,
            }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(-0.75, 1.0),
                rot: 0.0,
                shape: Box::new(ShapeDef::Box {
                    width: 0.5,
                    height: 1.5,
                }),
            }),
        }
    }

    /// The contour vertices that sit on the blend of [`filleted_l`].
    fn blend_vertices(radius: f32) -> usize {
        let corner = Vec2::new(-0.5, 0.25);
        polygonize(&filleted_l(radius))
            .outline
            .iter()
            .filter(|v| {
                let d = **v - corner;
                d.min_element() > -0.01 && d.max_element() < radius * 1.05
            })
            .count()
    }

    #[test]
    fn a_fillet_of_four_cells_resolves_into_a_curve() {
        // The contour samples the field every `extent / FIELD_CELLS`
        // (about 2.3 cm here). Vertices are refined onto the true blend,
        // so the chord error stays near a millimetre at every radius;
        // what the radius buys is how many vertices the arc gets. Below
        // ~2 cells the fillet reads as a chamfer (1-3 vertices); from
        // ~4 cells (0.1 m here) it is a curve.
        let (min, max) = sdf::aabb(&filleted_l(0.1));
        let cell = (max - min).max_element() / FIELD_CELLS;
        assert!(0.1 / cell >= 4.0, "0.1 m is {} cells", 0.1 / cell);
        assert!(blend_vertices(0.1) >= 5, "{}", blend_vertices(0.1));
        assert!(blend_vertices(0.3) >= 8, "{}", blend_vertices(0.3));
        // Every edge midpoint on the blend lies within 2 mm of it.
        let tree = filleted_l(0.3);
        let ring = polygonize(&tree).outline;
        let corner = Vec2::new(-0.5, 0.25);
        for (i, a) in ring.iter().enumerate() {
            let m = (*a + ring[(i + 1) % ring.len()]) / 2.0;
            if (m - corner).min_element() < 0.0 || (m - corner).length() > 0.45 {
                continue; // not on the blend
            }
            assert!(
                sdf::eval(&tree, m).abs() < 2e-3,
                "edge at {m} off the blend"
            );
        }
    }

    #[test]
    fn a_smooth_union_bridges_a_gap_under_half_its_radius() {
        // At the midpoint of a gap g between parallel faces both fields
        // are g/2, and the quadratic smooth-min lowers that by radius/4:
        // the blend closes the gap exactly when g < radius / 2.
        let pair = |gap: f32| ShapeDef::Csg {
            op: crate::shape::CsgOp::SmoothUnion { radius: 1.0 },
            lhs: Box::new(ShapeDef::Box {
                width: 1.0,
                height: 1.0,
            }),
            rhs: Box::new(ShapeDef::Placed {
                pos: Vec2::new(1.0 + gap, 0.0),
                rot: 0.0,
                shape: Box::new(ShapeDef::Box {
                    width: 1.0,
                    height: 1.0,
                }),
            }),
        };
        assert_eq!(polygonize_components(&pair(0.45)).len(), 1);
        assert_eq!(polygonize_components(&pair(0.55)).len(), 2);
    }
}
