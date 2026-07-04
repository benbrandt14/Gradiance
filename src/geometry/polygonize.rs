//! Converting authored shapes into [`Contours`].

use crate::core::constants::{CIRCLE_SEGMENTS, GROUND_SLAB_DEPTH, GROUND_SLAB_WIDTH};
use crate::domain::shape::ShapeDef;
use crate::geometry::contours::Contours;
use bevy::math::Vec2;

/// Converts a shape into explicit contours (counter-clockwise outline).
///
/// Circles are discretized with [`CIRCLE_SEGMENTS`] segments. The infinite
/// half-plane becomes a large finite slab (its rendered / CSG stand-in):
/// surface along local `y = 0`, extending [`GROUND_SLAB_DEPTH`] downward.
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
