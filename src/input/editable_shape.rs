//! Logic for live-editable shapes using a unified component.

use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::Point2;

/// Defines the type and dimensions of a shape.
#[derive(Debug, Clone, Reflect, PartialEq)]
pub enum ShapeType {
    /// A rectangle with width and height.
    Box {
        /// Width of the box.
        width: f32,
        /// Height of the box.
        height: f32,
    },
    /// A circle with a radius.
    Circle {
        /// Radius of the circle.
        radius: f32,
    },
    /// A polygon with a list of vertices.
    Polygon {
        /// List of vertices relative to the center.
        points: Vec<Vec2>,
    },
}

impl Default for ShapeType {
    fn default() -> Self {
        Self::Box {
            width: 1.0,
            height: 1.0,
        }
    }
}

/// A component representing a shape that can be edited.
///
/// Changing this component will automatically update the entity's
/// visual mesh and physics collider.
#[derive(Component, Reflect, Default, Debug, Clone, PartialEq)]
#[reflect(Component)]
pub struct EditableShape {
    /// The geometric definition of the shape.
    pub shape: ShapeType,
}

/// Plugin that handles updates to `EditableShape`.
pub struct EditableShapePlugin;

impl Plugin for EditableShapePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EditableShape>();
        app.add_systems(Update, update_shape_geometry);
    }
}

/// Helper function to generate geometry and collider from a shape type.
///
/// Returns `Some((Path, Collider))` if the shape is valid.
pub fn generate_shape_components(shape_type: &ShapeType) -> Option<(Path, Collider)> {
    match shape_type {
        ShapeType::Box { width, height } => {
            if *width <= 0.0 || *height <= 0.0 {
                return None;
            }
            let shape = shapes::Rectangle {
                extents: Vec2::new(*width, *height),
                origin: shapes::RectangleOrigin::Center,
                radii: None,
            };
            Some((
                GeometryBuilder::build_as(&shape),
                Collider::cuboid(width / 2.0, height / 2.0),
            ))
        }
        ShapeType::Circle { radius } => {
            if *radius <= 0.0 {
                return None;
            }
            let shape = shapes::Circle {
                radius: *radius,
                center: Vec2::ZERO,
            };
            Some((GeometryBuilder::build_as(&shape), Collider::ball(*radius)))
        }
        ShapeType::Polygon { points } => {
            if points.len() < 3 {
                return None;
            }
            let shape = shapes::Polygon {
                points: points.clone(),
                closed: true,
            };

            let vertices: Vec<Point2<f32>> = points.iter().map(|v| Point2::new(v.x, v.y)).collect();
            let indices: Vec<[u32; 2]> = (0..vertices.len())
                .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
                .collect();

            let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
            Some((
                GeometryBuilder::build_as(&shape),
                Collider::from(rapier_shape),
            ))
        }
    }
}

/// System to update the collider and visual shape when `EditableShape` changes.
fn update_shape_geometry(
    mut commands: Commands,
    query: Query<(Entity, &EditableShape), Changed<EditableShape>>,
) {
    for (entity, editable) in query.iter() {
        if let Some((path, collider)) = generate_shape_components(&editable.shape) {
            commands.entity(entity).insert(path).insert(collider);
        }
    }
}
