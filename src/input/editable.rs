//! Components for live-editable shapes.
//!
//! Provides `EditableBox` and `EditableCircle` components.

use crate::prelude::*;

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

/// Plugin that handles updates to editable shapes.
pub struct EditablePlugin;

impl Plugin for EditablePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EditableBox>();
        app.register_type::<EditableCircle>();
        app.add_systems(Update, (resize_box, resize_circle));
    }
}

/// System to update the collider and visual shape when `EditableBox` changes.
fn resize_box(
    mut commands: Commands,
    query: Query<(Entity, &EditableBox, Option<&Mesh2d>), Changed<EditableBox>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, editable, existing_mesh) in query.iter() {
        if editable.width <= 0.0 || editable.height <= 0.0 {
            continue;
        }

        // Clean up old mesh to prevent leaks (Assumes 1-to-1 ownership for editable objects)
        if let Some(mesh2d) = existing_mesh {
            meshes.remove(&mesh2d.0);
        }

        let mesh = Mesh::from(Rectangle::new(editable.width as f32, editable.height as f32));
        let handle = meshes.add(mesh);

        commands
            .entity(entity)
            .insert(Mesh2d(handle))
            .insert(Collider::cuboid(
                (editable.width / 2.0) as f32,
                (editable.height / 2.0) as f32,
            ));
    }
}

/// System to update the collider and visual shape when `EditableCircle` changes.
fn resize_circle(
    mut commands: Commands,
    query: Query<(Entity, &EditableCircle, Option<&Mesh2d>), Changed<EditableCircle>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (entity, editable, existing_mesh) in query.iter() {
        if editable.radius <= 0.0 {
            continue;
        }

        // Clean up old mesh
        if let Some(mesh2d) = existing_mesh {
            meshes.remove(&mesh2d.0);
        }

        let mesh = Mesh::from(Circle::new(editable.radius as f32));
        let handle = meshes.add(mesh);

        commands
            .entity(entity)
            .insert(Mesh2d(handle))
            .insert(Collider::ball(editable.radius as f32));
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
}
