//! The authored geometry of a body.

use bevy::math::Vec2;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// Minimum linear dimension accepted for authored shapes, in world pixels.
pub const MIN_SHAPE_SIZE: f32 = 0.01;

/// The engine-agnostic source-of-truth geometry of a body.
///
/// Editing this component live-regenerates the collider and the rendered
/// mesh via the physics/render sync systems. Polygon vertices are
/// **centroid-relative**; `holes` are produced by CSG cuts.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeDef {
    /// An axis-aligned rectangle (before body rotation).
    Box {
        /// Full width in world pixels.
        width: f32,
        /// Full height in world pixels.
        height: f32,
    },
    /// A circle.
    Circle {
        /// Radius in world pixels.
        radius: f32,
    },
    /// An arbitrary simple polygon, possibly with holes.
    Polygon {
        /// Outer boundary, centroid-relative, counter-clockwise.
        outline: Vec<Vec2>,
        /// Interior holes, each clockwise.
        holes: Vec<Vec<Vec2>>,
    },
}

/// Why a [`ShapeDef`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShapeError {
    /// A box dimension or circle radius was non-positive or too small.
    #[error("shape dimensions must be at least {MIN_SHAPE_SIZE} px")]
    Degenerate,
    /// A polygon outline had fewer than three vertices.
    #[error("polygon needs at least 3 vertices, got {0}")]
    TooFewVertices(usize),
    /// A coordinate was NaN or infinite.
    #[error("shape contains a non-finite coordinate")]
    NonFinite,
}

impl ShapeDef {
    /// Validates the shape, returning it untouched when acceptable.
    ///
    /// Commands must refuse to spawn or update bodies with invalid shapes.
    pub fn validate(&self) -> Result<(), ShapeError> {
        match self {
            Self::Box { width, height } => {
                if !(width.is_finite() && height.is_finite()) {
                    return Err(ShapeError::NonFinite);
                }
                if *width < MIN_SHAPE_SIZE || *height < MIN_SHAPE_SIZE {
                    return Err(ShapeError::Degenerate);
                }
                Ok(())
            }
            Self::Circle { radius } => {
                if !radius.is_finite() {
                    return Err(ShapeError::NonFinite);
                }
                if *radius < MIN_SHAPE_SIZE {
                    return Err(ShapeError::Degenerate);
                }
                Ok(())
            }
            Self::Polygon { outline, holes } => {
                if outline.len() < 3 {
                    return Err(ShapeError::TooFewVertices(outline.len()));
                }
                let all = outline.iter().chain(holes.iter().flatten());
                for v in all {
                    if !v.is_finite() {
                        return Err(ShapeError::NonFinite);
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_and_circles_validate_by_size() {
        assert!(
            ShapeDef::Box {
                width: 10.0,
                height: 5.0
            }
            .validate()
            .is_ok()
        );
        assert_eq!(
            ShapeDef::Box {
                width: 0.0,
                height: 5.0
            }
            .validate(),
            Err(ShapeError::Degenerate)
        );
        assert_eq!(
            ShapeDef::Circle { radius: -1.0 }.validate(),
            Err(ShapeError::Degenerate)
        );
    }

    #[test]
    fn polygons_need_three_finite_vertices() {
        let tri = ShapeDef::Polygon {
            outline: vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(0.0, 1.0),
            ],
            holes: vec![],
        };
        assert!(tri.validate().is_ok());

        let two = ShapeDef::Polygon {
            outline: vec![Vec2::ZERO, Vec2::X],
            holes: vec![],
        };
        assert_eq!(two.validate(), Err(ShapeError::TooFewVertices(2)));

        let nan = ShapeDef::Polygon {
            outline: vec![Vec2::new(f32::NAN, 0.0), Vec2::X, Vec2::Y],
            holes: vec![],
        };
        assert_eq!(nan.validate(), Err(ShapeError::NonFinite));
    }
}
