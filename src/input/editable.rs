//! Components for live-editable shapes.
//!
//! Provides `EditableBox` and `EditableCircle` components.

use crate::prelude::*;
use crate::geometry::gear::GearProfile;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::Point2;

/// A component representing a box that can be resized.
#[derive(Component, Reflect, Default, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct EditableBox {
    /// The width of the box.
    pub width: f64,
    /// The height of the box.
    pub height: f64,
}

/// A component representing a circle that can be resized.
#[derive(Component, Reflect, Default, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct EditableCircle {
    /// The radius of the circle.
    pub radius: f64,
}

/// A component representing a gear that can be modified.
#[derive(Component, Reflect, Default, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct EditableGear {
    /// The profile of the gear.
    pub profile: GearProfile,
}

/// Plugin that handles updates to editable shapes.
pub struct EditablePlugin;

impl Plugin for EditablePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EditableBox>();
        app.register_type::<EditableCircle>();
        app.register_type::<EditableGear>();
        app.add_systems(Update, (resize_box, resize_circle, resize_gear));
    }
}

/// System to update the collider and visual shape when `EditableBox` changes.
fn resize_box(mut commands: Commands, query: Query<(Entity, &EditableBox), Changed<EditableBox>>) {
    for (entity, editable) in query.iter() {
        if editable.width <= 0.0 || editable.height <= 0.0 {
            continue;
        }

        let shape = shapes::Rectangle {
            extents: Vec2::new(editable.width as f32, editable.height as f32),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        commands
            .entity(entity)
            .insert(GeometryBuilder::build_as(&shape))
            .insert(Collider::cuboid(
                (editable.width / 2.0) as f32,
                (editable.height / 2.0) as f32,
            ));
    }
}

/// System to update the collider and visual shape when `EditableCircle` changes.
fn resize_circle(
    mut commands: Commands,
    query: Query<(Entity, &EditableCircle), Changed<EditableCircle>>,
) {
    for (entity, editable) in query.iter() {
        if editable.radius <= 0.0 {
            continue;
        }

        let shape = shapes::Circle {
            radius: editable.radius as f32,
            center: Vec2::ZERO,
        };

        commands
            .entity(entity)
            .insert(GeometryBuilder::build_as(&shape))
            .insert(Collider::ball(editable.radius as f32));
    }
}

/// System to update the collider and visual shape when `EditableGear` changes.
fn resize_gear(
    mut commands: Commands,
    query: Query<(Entity, &EditableGear), Changed<EditableGear>>,
) {
    for (entity, editable) in query.iter() {
        let path = editable.profile.build_path();

        // Generate points for collider
        // Use lower resolution for physics to save performance if needed,
        // but gears need precision. GearProfile default is 10.
        let points = editable.profile.generate_points(5);

        if points.len() < 3 {
            continue;
        }

        let vertices: Vec<Point2<f32>> = points
            .iter()
            .map(|v| Point2::new(v.x, v.y))
            .collect();

        // For concave shapes like gears, we must use convex decomposition or polyline.
        // Convex decomposition is solid. Polyline is hollow.
        // Polyline might cause tunneling if thin?
        // TriMesh is also an option for static, but for dynamic?
        // SharedShape::convex_decomposition handles concave polygons.
        let indices: Vec<[u32; 2]> = (0..vertices.len())
            .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
            .collect();

        // Use convex decomposition for solid physics
        let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
        let collider = Collider::from(rapier_shape);

        commands
            .entity(entity)
            .insert(ShapeBundle {
                path,
                ..default()
            })
            .insert(collider);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editable_box_creation() {
        let e = EditableBox {
            width: 10.0,
            height: 20.0,
        };
        assert_eq!(e.width, 10.0);
        assert_eq!(e.height, 20.0);
    }

    #[test]
    fn test_editable_circle_creation() {
        let e = EditableCircle { radius: 5.0 };
        assert_eq!(e.radius, 5.0);
    }

    #[test]
    fn test_editable_gear_creation() {
        let e = EditableGear::default();
        assert_eq!(e.profile.teeth, 12);
    }
}
