//! Converting authored shapes into [`Contours`] — the **single
//! discretization point** between SDF trees and every polygon consumer
//! (colliders, meshes, snapping, scaling).

use crate::core::constants::{CIRCLE_SEGMENTS, GROUND_SLAB_DEPTH, GROUND_SLAB_WIDTH};
use crate::domain::shape::ShapeDef;
use crate::geometry::contour::{FIELD_CELLS, contour_components};
use crate::geometry::contours::Contours;
use crate::geometry::sdf;
use bevy::math::Vec2;

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
    use crate::geometry::contours::ring_signed_area;

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
}
