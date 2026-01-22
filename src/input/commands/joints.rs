//! Commands for spawning joints (Revolute, Fixed, Prismatic, Spring, Rope).

use crate::input::commands::GameCommand;
use crate::input::tools::connector::Connector;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::dynamics::TypedJoint;
use bevy_rapier2d::rapier::dynamics::{
    FixedJointBuilder as RapierFixedJointBuilder, GenericJoint,
    PrismaticJointBuilder as RapierPrismaticJointBuilder,
    RevoluteJointBuilder as RapierRevoluteJointBuilder,
    RopeJointBuilder as RapierRopeJointBuilder, SpringJointBuilder as RapierSpringJointBuilder,
};
use nalgebra::{Isometry2, Point2, Vector2};

/// Helper to resolve joint targets and handle pinning.
fn resolve_joint_targets(
    world: &mut World,
    _entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    visual_entity: Option<Entity>,
) -> (Entity, Option<Entity>, Vec2, Vec2) {
    let mut pin_entity = None;
    let target_entity;
    let local_anchor_1 = anchor_a;
    let local_anchor_2;

    if let Some(e_b) = entity_b {
        target_entity = e_b;
        local_anchor_2 = anchor_b;
    } else {
        // Pin logic
        // If pinning to world (entity_b is None), anchor_b represents the world position of the pin.
        // We spawn the pin at that world position.
        let world_pos = Vec3::new(anchor_b.x, anchor_b.y, 0.0);

        let pin_id = world
            .spawn((RigidBody::Fixed, Transform::from_translation(world_pos)))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity
            && let Some(mut connector) = world.get_mut::<Connector>(v_id)
        {
            connector.entity_b = Some(pin_id);
        }
    }

    (
        target_entity,
        pin_entity,
        local_anchor_1,
        local_anchor_2,
    )
}

/// Helper to spawn connector visual.
fn spawn_connector_visual(
    world: &mut World,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_a: Vec2,
    anchor_b: Vec2,
    children_builder: impl FnOnce(&mut World, Entity),
) -> Entity {
    let visual_id = world
        .spawn((
            Transform::from_xyz(anchor_a.x, anchor_a.y, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::ball(0.5),
            Sensor,
            Connector {
                entity_a,
                entity_b,
                local_anchor_a: anchor_a,
                local_anchor_b: anchor_b,
            },
        ))
        .set_parent_in_place(entity_a)
        .id();

    children_builder(world, visual_id);

    visual_id
}

/// Command to spawn a Revolute Joint (Hinge).
pub struct SpawnRevoluteJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional, None means World/Pin).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnRevoluteJointCommand {
    fn name(&self) -> String {
        "Spawn Revolute Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        // Visual
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                let circle_outer = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 5.0,
                    ..default()
                });
                world.entity_mut(visual_id).insert((
                    ShapeBundle {
                        path: circle_outer,
                        ..default()
                    },
                    Fill::color(Color::BLACK),
                ));

                let circle_inner = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 2.0,
                    ..default()
                });
                let inner = world
                    .spawn((
                        ShapeBundle {
                            path: circle_inner,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Fill::color(Color::WHITE),
                    ))
                    .id();
                world.entity_mut(visual_id).add_child(inner);
            },
        );
        self.visual_entity = Some(visual_id);

        // Physics Joint
        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let builder = RapierRevoluteJointBuilder::new()
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }
        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

/// Command to spawn a Fixed Joint (Weld).
pub struct SpawnFixedJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
    /// Rotation of body A (radians).
    pub rot_a: f32,
    /// Rotation of body B (radians).
    pub rot_b: f32,
}

impl GameCommand for SpawnFixedJointCommand {
    fn name(&self) -> String {
        "Spawn Fixed Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                let line1 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-3.0, -3.0),
                    Vec2::new(3.0, 3.0),
                ));
                let v1 = world
                    .spawn((
                        ShapeBundle {
                            path: line1,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
                    ))
                    .id();

                let line2 = GeometryBuilder::build_as(&shapes::Line(
                    Vec2::new(-3.0, 3.0),
                    Vec2::new(3.0, -3.0),
                ));
                let v2 = world
                    .spawn((
                        ShapeBundle {
                            path: line2,
                            transform: Transform::from_translation(Vec3::Z * 0.1),
                            ..default()
                        },
                        Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0),
                    ))
                    .id();
                world.entity_mut(visual_id).add_children(&[v1, v2]);
            },
        );
        self.visual_entity = Some(visual_id);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let frame2 = Isometry2::new(
            Vector2::new(local_anchor_2.x, local_anchor_2.y),
            self.rot_a - self.rot_b,
        );
        let builder = RapierFixedJointBuilder::new()
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_frame2(frame2);
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }
        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

/// Command to spawn a Prismatic Joint.
pub struct SpawnPrismaticJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Axis of the slider.
    pub axis: Vec2,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnPrismaticJointCommand {
    fn name(&self) -> String {
        "Spawn Prismatic Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                // Visual: A box or slider
                let rect = GeometryBuilder::build_as(&shapes::Rectangle {
                    extents: Vec2::new(8.0, 4.0),
                    ..default()
                });
                world.entity_mut(visual_id).insert((
                    ShapeBundle {
                        path: rect,
                        ..default()
                    },
                    Stroke::new(Color::srgb(0.0, 0.0, 1.0), 1.0),
                ));
            },
        );
        self.visual_entity = Some(visual_id);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let axis = Vector2::new(self.axis.x, self.axis.y);
        let unit_axis = nalgebra::Unit::new_normalize(axis);
        let builder = RapierPrismaticJointBuilder::new(unit_axis)
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }
        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

/// Command to spawn a Spring Joint.
pub struct SpawnSpringJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Stiffness of the spring.
    pub stiffness: f32,
    /// Damping of the spring.
    pub damping: f32,
    /// Rest length of the spring.
    pub length: f32,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnSpringJointCommand {
    fn name(&self) -> String {
        "Spawn Spring Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                // Visual: A zig-zag or line. We use connector's default line update for now?
                // The connector component updates visuals, but we can add a sprite here.
                // For now, simple green circle at anchor.
                let circle = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 3.0,
                    ..default()
                });
                world.entity_mut(visual_id).insert((
                    ShapeBundle {
                        path: circle,
                        ..default()
                    },
                    Fill::color(Color::srgb(0.0, 1.0, 0.0)),
                ));
            },
        );
        self.visual_entity = Some(visual_id);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let builder = RapierSpringJointBuilder::new(self.length, self.stiffness, self.damping)
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }
        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
    }
}

/// Command to spawn a Rope Joint.
pub struct SpawnRopeJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Max length of the rope.
    pub length: f32,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID.
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnRopeJointCommand {
    fn name(&self) -> String {
        "Spawn Rope Joint".to_string()
    }

    fn apply(&mut self, world: &mut World) -> Result<(), String> {
        let visual_id = spawn_connector_visual(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            |world, visual_id| {
                let circle = GeometryBuilder::build_as(&shapes::Circle {
                    radius: 3.0,
                    ..default()
                });
                world.entity_mut(visual_id).insert((
                    ShapeBundle {
                        path: circle,
                        ..default()
                    },
                    Fill::color(Color::srgb(1.0, 0.5, 0.0)),
                ));
            },
        );
        self.visual_entity = Some(visual_id);

        let (target_entity, pin_entity, local_anchor_1, local_anchor_2) = resolve_joint_targets(
            world,
            self.entity_a,
            self.entity_b,
            self.anchor_a,
            self.anchor_b,
            Some(visual_id),
        );
        self.pin_entity = pin_entity;

        let builder = RapierRopeJointBuilder::new(self.length)
            .local_anchor1(Point2::new(local_anchor_1.x, local_anchor_1.y))
            .local_anchor2(Point2::new(local_anchor_2.x, local_anchor_2.y));
        let mut joint_data = GenericJoint::from(builder);
        joint_data.set_contacts_enabled(false);

        world.entity_mut(self.entity_a).insert(ImpulseJoint::new(
            target_entity,
            TypedJoint::GenericJoint(bevy_rapier2d::dynamics::GenericJoint { raw: joint_data }),
        ));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }
        if let Ok(mut e) = world.get_entity_mut(self.entity_a) {
            e.remove::<ImpulseJoint>();
        }
        if let Some(p) = self.pin_entity {
            if let Ok(e) = world.get_entity_mut(p) {
                e.despawn();
            }
            self.pin_entity = None;
        }
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
    fn test_spawn_revolute_joint_command(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnRevoluteJointCommand {
            entity_a,
            entity_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            visual_entity: None,
            pin_entity: None,
        };

        assert!(cmd.apply(&mut world).is_ok());
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        cmd.undo(&mut world);
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());
    }

    // Additional tests for other joints can be added here
    #[rstest]
    fn test_spawn_prismatic_joint_command(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnPrismaticJointCommand {
            entity_a,
            entity_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            axis: Vec2::Y,
            visual_entity: None,
            pin_entity: None,
        };

        assert!(cmd.apply(&mut world).is_ok());
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        cmd.undo(&mut world);
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());
    }

    #[rstest]
    fn test_spawn_spring_joint_pin_location(mut world: World) {
        // Body A at (0,0)
        let entity_a = world.spawn((Transform::default(), GlobalTransform::default())).id();

        // Target pin location at (10, 10) in world space
        let pin_loc = Vec2::new(10.0, 10.0);

        let mut cmd = SpawnSpringJointCommand {
            entity_a,
            entity_b: None,
            anchor_a: Vec2::ZERO, // Local to A
            anchor_b: pin_loc,    // World location (since B is None)
            stiffness: 10.0,
            damping: 0.5,
            length: 10.0,
            visual_entity: None,
            pin_entity: None,
        };

        assert!(cmd.apply(&mut world).is_ok());

        let joint = world.get::<ImpulseJoint>(entity_a).unwrap();
        let pin_entity = joint.parent;
        let pin_transform = world.get::<Transform>(pin_entity).unwrap();

        // Expect pin to be at (10, 10)
        assert!((pin_transform.translation.x - 10.0).abs() < 1e-5);
        assert!((pin_transform.translation.y - 10.0).abs() < 1e-5);
    }
}
