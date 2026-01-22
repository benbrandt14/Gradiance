//! Commands for spawning shapes (Box, Circle, Polygon, Ground).

use crate::input::ZIndex;
use crate::input::commands::{GameCommand, undo_despawn_recursive};
use crate::input::editable::{EditableBox, EditableCircle};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::rapier::geometry::SharedShape;
use nalgebra::Point2;

/// Helper to spawn a shape entity with common components.
fn spawn_shape_entity(
    world: &mut World,
    position: Vec2,
    shape: &impl bevy_prototype_lyon::geometry::Geometry,
    collider: Collider,
    fill: Fill,
    extra_bundle: impl Bundle,
) -> Entity {
    let z = world.resource_mut::<ZIndex>().next();

    world
        .spawn((
            ShapeBundle {
                path: GeometryBuilder::build_as(shape),
                transform: Transform::from_xyz(position.x, position.y, z),
                ..default()
            },
            fill,
            Stroke::new(Color::srgb(0.0, 0.0, 0.0), 0.1),
            RigidBody::Dynamic,
            collider,
            extra_bundle,
        ))
        .id()
}

/// Command to spawn a box.
pub struct SpawnBoxCommand {
    /// Position of the box.
    pub position: Vec2,
    /// Width of the box.
    pub width: f32,
    /// Height of the box.
    pub height: f32,
    /// The spawned entity ID (if active).
    pub entity: Option<Entity>,
}

impl SpawnBoxCommand {
    /// Create a new SpawnBoxCommand.
    pub fn new(position: Vec2, width: f32, height: f32) -> Self {
        Self {
            position,
            width,
            height,
            entity: None,
        }
    }
}

impl GameCommand for SpawnBoxCommand {
    fn name(&self) -> String {
        "Spawn Box".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let shape = shapes::Rectangle {
            extents: Vec2::new(self.width, self.height),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            Collider::cuboid(self.width / 2.0, self.height / 2.0),
            Fill::color(Color::srgb(0.5, 0.5, 1.0)),
            EditableBox {
                width: self.width as f64,
                height: self.height as f64,
            },
        );

        self.entity = Some(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        undo_despawn_recursive(world, &mut self.entity);
    }
}

/// Command to spawn a circle.
pub struct SpawnCircleCommand {
    /// The center position.
    pub position: Vec2,
    /// The radius.
    pub radius: f32,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnCircleCommand {
    fn name(&self) -> String {
        "Spawn Circle".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let shape = shapes::Circle {
            radius: self.radius,
            center: Vec2::ZERO,
        };

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            Collider::ball(self.radius),
            Fill {
                color: Color::srgb(1.0, 0.5, 0.5),
                options: FillOptions::default().with_tolerance(0.001),
            },
            EditableCircle {
                radius: self.radius as f64,
            },
        );

        self.entity = Some(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        undo_despawn_recursive(world, &mut self.entity);
    }
}

/// Command to spawn a polygon.
pub struct SpawnPolygonCommand {
    /// The center position.
    pub position: Vec2,
    /// The vertices relative to the center.
    pub vertices: Vec<Vec2>,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnPolygonCommand {
    fn name(&self) -> String {
        "Spawn Polygon".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        if self.vertices.len() < 3 {
            return Err("Polygon must have at least 3 vertices".to_string());
        }

        let shape = shapes::Polygon {
            points: self.vertices.clone(),
            closed: true,
        };

        let vertices: Vec<Point2<f32>> = self
            .vertices
            .iter()
            .map(|v| Point2::new(v.x, v.y))
            .collect();
        let indices: Vec<[u32; 2]> = (0..vertices.len())
            .map(|i| [i as u32, ((i + 1) % vertices.len()) as u32])
            .collect();

        let rapier_shape = SharedShape::convex_decomposition(&vertices, &indices);
        let collider = Collider::from(rapier_shape);

        let entity = spawn_shape_entity(
            world,
            self.position,
            &shape,
            collider,
            Fill::color(Color::srgb(0.5, 1.0, 0.5)),
            (),
        );

        self.entity = Some(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        undo_despawn_recursive(world, &mut self.entity);
    }
}

/// Command to spawn an infinite ground plane.
pub struct SpawnGroundCommand {
    /// Position of the ground (center of surface).
    pub position: Vec2,
    /// Rotation angle (radians).
    pub rotation: f32,
    /// The spawned entity ID.
    pub entity: Option<Entity>,
}

impl GameCommand for SpawnGroundCommand {
    fn name(&self) -> String {
        "Spawn Ground".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let width = 100_000.0;
        let depth = 1000.0;

        // Visual shape: Huge rectangle
        let shape = shapes::Rectangle {
            extents: Vec2::new(width, depth),
            origin: shapes::RectangleOrigin::Center,
            radii: None,
        };

        // Offset: the visual rectangle is centered at (0,0).
        // The collider is centered at (0,0).
        // But we want the "surface" (top edge) to be at `position` aligned with `rotation`.
        // The box center is at (0, -depth/2) relative to the surface.
        // So we rotate (0, -depth/2) by `rotation` and add to `position`.

        let rot = Quat::from_rotation_z(self.rotation);
        let offset = rot * Vec3::new(0.0, -depth / 2.0, 0.0);
        let center = Vec3::new(self.position.x, self.position.y, 0.0) + offset;

        let z = -1.0; // Force ground to be behind

        let entity = world
            .spawn((
                // Shape Bundle
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shape),
                    transform: Transform::from_translation(center + Vec3::new(0.0, 0.0, z))
                        .with_rotation(rot),
                    ..default()
                },
                Fill::color(Color::srgb(0.2, 0.2, 0.2)), // Dark grey
                Stroke::new(Color::srgb(0.0, 0.0, 0.0), 2.0),
                // Physics
                RigidBody::Fixed,
                Collider::cuboid(width / 2.0, depth / 2.0),
                Friction::coefficient(0.5),
                Restitution::coefficient(0.0),
                GroundPlane,
                Name::new("Ground"),
            ))
            .id();

        self.entity = Some(entity);
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        undo_despawn_recursive(world, &mut self.entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ZIndex as GameZIndex;
    use rstest::{fixture, rstest};

    #[fixture]
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameZIndex>();
        world
    }

    #[rstest]
    fn test_spawn_polygon_command_failure(mut world: World) {
        let vertices = vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]; // Only 2 vertices
        let mut cmd = SpawnPolygonCommand {
            position: Vec2::new(0.0, 0.0),
            vertices: vertices.clone(),
            entity: None,
        };

        // Apply should fail
        let result = cmd.apply(&mut world);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Polygon must have at least 3 vertices");
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_box_command(mut world: World) {
        let mut cmd = SpawnBoxCommand::new(Vec2::new(10.0, 20.0), 5.0, 5.0);

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(10.0, 20.0)
        );

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableBox>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        // In Bevy 0.15+, get_entity returns a Result. If despawned, it should be Err.
        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_circle_command(mut world: World) {
        let mut cmd = SpawnCircleCommand {
            position: Vec2::new(-5.0, 5.0),
            radius: 3.0,
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        let transform = world.get::<Transform>(entity);
        assert!(transform.is_some());
        assert_eq!(
            transform.unwrap().translation.truncate(),
            Vec2::new(-5.0, 5.0)
        );

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<EditableCircle>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_polygon_command(mut world: World) {
        let vertices = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 10.0),
        ];
        let mut cmd = SpawnPolygonCommand {
            position: Vec2::new(0.0, 0.0),
            vertices: vertices.clone(),
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        // Verify shape bundle exists (checking Transform as proxy)
        assert!(world.get::<Transform>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
        assert!(cmd.entity.is_none());
    }

    #[rstest]
    fn test_spawn_ground_command(mut world: World) {
        let mut cmd = SpawnGroundCommand {
            position: Vec2::new(10.0, 10.0),
            rotation: 0.0,
            entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        assert!(cmd.entity.is_some());
        let entity = cmd.entity.unwrap();

        assert!(world.get::<RigidBody>(entity).is_some());
        assert!(world.get::<Collider>(entity).is_some());
        assert!(world.get::<GroundPlane>(entity).is_some());
        assert!(world.get::<Transform>(entity).is_some());

        // Undo
        cmd.undo(&mut world);

        assert!(world.get_entity(entity).is_err());
    }
}
