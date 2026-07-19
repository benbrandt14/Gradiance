//! The authored geometry of a body.
//!
//! `ShapeDef` is a **signed-distance-function tree**: analytic leaves
//! (`Box`, `Circle`, `Polygon`, `HalfPlane`) composed by CSG operators
//! (`Csg`) and rigid placements (`Placed`). Leaves polygonize exactly;
//! operator trees are contoured from the field (`geometry::contour`).
//! Every derived consumer — colliders, meshes, snapping — reads the tree
//! through one adapter (`geometry::polygonize`), so the representation
//! has a single discretization point.

use bevy::math::Vec2;
use bevy::prelude::Component;
use bevy::reflect::{ReflectDeserialize, ReflectSerialize};
use serde::{Deserialize, Serialize};

/// Minimum linear dimension accepted for authored shapes, in world pixels.
pub const MIN_SHAPE_SIZE: f32 = 0.01;

/// Maximum CSG tree depth before commands must bake results to a
/// [`ShapeDef::Polygon`] leaf (bounds evaluation cost under repeated cuts).
pub const MAX_CSG_DEPTH: u32 = 8;

/// A CSG boolean operator over two SDF subtrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CsgOp {
    /// Material in either operand (`min` of fields).
    Union,
    /// Material of `lhs` minus material of `rhs` (`max(a, -b)`).
    Subtract,
    /// Material in both operands (`max` of fields).
    Intersect,
    /// Union with a fillet of the given radius (polynomial smooth-min) —
    /// blends the operands where they meet.
    SmoothUnion {
        /// Blend radius in world pixels (> 0).
        radius: f32,
    },
}

/// The engine-agnostic source-of-truth geometry of a body.
///
/// Editing this component live-regenerates the collider and the rendered
/// mesh via the physics/render sync systems. Polygon vertices are
/// **centroid-relative at authoring time** (CSG reshapes may leave the
/// body origin off-centroid); `holes` are produced by CSG cuts.
///
/// Reflects as an **opaque** value (`#[reflect(opaque)]`), the ratified design
/// for scripting: `ShapeDef` is a foreign value built by geometry constructor
/// builtins and passed as a handle, never authored field-by-field through
/// reflection (see `docs/script-lisp-decision.md`). Opacity insulates the
/// scripting surface from the SDF enum's churn and lets records that embed a
/// `ShapeDef` (`BodyRecord`, `PropertyValue::Shape`) derive `Reflect`. Script
/// tree-walking, if ever wanted, is additive later via accessor builtins.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
#[reflect(opaque, Serialize, Deserialize)]
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
    /// A capsule (stadium): the segment from `(-half_length, 0)` to
    /// `(+half_length, 0)` inflated by `radius`. The rod tool's shape — a
    /// visually 1D rigid rod whose collider is the exact inflated segment.
    Capsule {
        /// Half the segment length in world pixels (rod length = 2·`half_length`).
        half_length: f32,
        /// Inflation radius (half the drawn thickness) in world pixels.
        radius: f32,
    },
    /// An infinite half-plane whose surface passes through the body origin.
    ///
    /// The solid side is local −Y (surface normal +Y); the body's rotation
    /// orients it. Used by the ground tool; physically boundless, rendered
    /// as a large slab.
    HalfPlane,
    /// A CSG boolean over two subtrees (fields in this shape's space).
    Csg {
        /// The operator.
        op: CsgOp,
        /// Left operand.
        lhs: Box<ShapeDef>,
        /// Right operand.
        rhs: Box<ShapeDef>,
    },
    /// A subtree rigidly placed within this shape's space (rotation and
    /// translation only — isometries keep the field an exact distance).
    Placed {
        /// Subtree origin in this shape's space.
        pos: Vec2,
        /// Subtree rotation, radians.
        rot: f32,
        /// The placed subtree.
        shape: Box<ShapeDef>,
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
    /// A short type label for the shape — the node-graph block title and other
    /// terse readouts. Composite trees report their outermost operator.
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Box { .. } => "box",
            Self::Circle { .. } => "circle",
            Self::Polygon { .. } => "polygon",
            Self::Capsule { .. } => "rod",
            Self::HalfPlane => "plane",
            Self::Csg { .. } => "csg",
            Self::Placed { .. } => "shape",
        }
    }

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
            Self::Capsule {
                half_length,
                radius,
            } => {
                if !(half_length.is_finite() && radius.is_finite()) {
                    return Err(ShapeError::NonFinite);
                }
                if *half_length < MIN_SHAPE_SIZE || *radius < MIN_SHAPE_SIZE {
                    return Err(ShapeError::Degenerate);
                }
                Ok(())
            }
            Self::HalfPlane => Ok(()),
            Self::Csg { op, lhs, rhs } => {
                if let CsgOp::SmoothUnion { radius } = op
                    && !(radius.is_finite() && *radius > 0.0)
                {
                    return Err(ShapeError::Degenerate);
                }
                lhs.validate()?;
                rhs.validate()
            }
            Self::Placed { pos, rot, shape } => {
                if !(pos.is_finite() && rot.is_finite()) {
                    return Err(ShapeError::NonFinite);
                }
                shape.validate()
            }
        }
    }

    /// Depth of the CSG tree (leaves are 1).
    pub fn depth(&self) -> u32 {
        match self {
            Self::Box { .. }
            | Self::Circle { .. }
            | Self::Polygon { .. }
            | Self::Capsule { .. }
            | Self::HalfPlane => 1,
            Self::Csg { lhs, rhs, .. } => 1 + lhs.depth().max(rhs.depth()),
            Self::Placed { shape, .. } => 1 + shape.depth(),
        }
    }

    /// Whether any leaf of this tree is the infinite ground half-plane.
    pub fn contains_half_plane(&self) -> bool {
        match self {
            Self::HalfPlane => true,
            Self::Box { .. }
            | Self::Circle { .. }
            | Self::Polygon { .. }
            | Self::Capsule { .. } => false,
            Self::Csg { lhs, rhs, .. } => lhs.contains_half_plane() || rhs.contains_half_plane(),
            Self::Placed { shape, .. } => shape.contains_half_plane(),
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
