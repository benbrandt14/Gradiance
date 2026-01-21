//! Commands for spawning joints (Revolute, Fixed).

use crate::input::commands::GameCommand;
use crate::input::tools::connector::Connector;
use crate::prelude::*;
use bevy_prototype_lyon::prelude::*;

/// Helper to resolve joint targets and handle pinning.
fn resolve_joint_targets(
    world: &mut World,
    entity_a: Entity,
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
        let t_a = world
            .get::<GlobalTransform>(entity_a)
            .map(|t| t.compute_transform())
            .unwrap_or_default();
        let world_pos = t_a.transform_point(Vec3::new(anchor_a.x, anchor_a.y, 0.0));

        let pin_id = world
            .spawn((RigidBody::Fixed, Transform::from_translation(world_pos)))
            .id();

        pin_entity = Some(pin_id);
        target_entity = pin_id;
        local_anchor_2 = Vec2::ZERO;

        if let Some(v_id) = visual_entity
            && let Some(mut connector) = world.get_mut::<Connector>(v_id) {
                connector.entity_b = Some(pin_id);
            }
    }

    (target_entity, pin_entity, local_anchor_1, local_anchor_2)
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
pub struct SpawnJointCommand {
    /// The first body.
    pub entity_a: Entity,
    /// The second body (optional, None means World/Pin).
    pub entity_b: Option<Entity>,
    /// Anchor on body A (local).
    pub anchor_a: Vec2,
    /// Anchor on body B (local).
    pub anchor_b: Vec2,
    /// Joint compliance (ignored in rigid Rapier joints usually, or mapped to stiffness).
    pub compliance: f32,
    /// The visual entity ID.
    pub visual_entity: Option<Entity>,
    /// The pin entity ID (if pinning to world).
    pub pin_entity: Option<Entity>,
}

impl GameCommand for SpawnJointCommand {
    fn name(&self) -> String {
        "Spawn Joint".to_string()
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

        let joint_data = RevoluteJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2);

        // Attach ImpulseJoint to entity_a
        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
        Ok(())
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(v) = self.visual_entity {
            if let Ok(e) = world.get_entity_mut(v) {
                e.despawn();
            }
            self.visual_entity = None;
        }

        // Remove Joint from entity_a
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
    /// Joint compliance.
    pub compliance: f32,
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

        let joint_data = FixedJointBuilder::new()
            .local_anchor1(local_anchor_1)
            .local_anchor2(local_anchor_2);

        world
            .entity_mut(self.entity_a)
            .insert(ImpulseJoint::new(target_entity, joint_data));
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
    use crate::input::tools::connector::Connector;
    use rstest::{fixture, rstest};

    #[fixture]
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<GameZIndex>();
        world
    }

    #[rstest]
    fn test_spawn_joint_command(mut world: World) {
        // Setup entity_a
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnJointCommand {
            entity_a,
            entity_b: None, // Pin to world
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            visual_entity: None,
            pin_entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint on entity_a
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Check visual entity spawned (child of entity_a)
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        // Since we don't know if there are other children, we look for one with Connector
        let visual_id = children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some());
        assert!(visual_id.is_some());
        let visual_id = *visual_id.unwrap();

        // Check pin entity
        assert!(cmd.pin_entity.is_some());
        let pin_id = cmd.pin_entity.unwrap();
        assert!(world.get::<RigidBody>(pin_id).is_some());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check visual entity despawned
        assert!(world.get_entity(visual_id).is_err());

        // Check pin entity despawned
        assert!(world.get_entity(pin_id).is_err());
    }

    #[rstest]
    fn test_spawn_fixed_joint_command(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();

        let mut cmd = SpawnFixedJointCommand {
            entity_a,
            entity_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            visual_entity: None,
            pin_entity: None,
            rot_a: 0.0,
            rot_b: 0.0,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Check visual entity
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        let visual_id = *children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some())
            .unwrap();

        // Check pin entity
        assert!(cmd.pin_entity.is_some());
        let pin_id = cmd.pin_entity.unwrap();
        assert!(world.get::<RigidBody>(pin_id).is_some());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check entities despawned
        assert!(world.get_entity(pin_id).is_err());
        assert!(world.get_entity(visual_id).is_err());
    }

    #[rstest]
    fn test_spawn_joint_command_two_bodies(mut world: World) {
        let entity_a = world.spawn(Transform::default()).id();
        let entity_b = world.spawn(Transform::default()).id();

        let mut cmd = SpawnJointCommand {
            entity_a,
            entity_b: Some(entity_b),
            anchor_a: Vec2::ZERO,
            anchor_b: Vec2::ZERO,
            compliance: 0.0,
            visual_entity: None,
            pin_entity: None,
        };

        // Apply
        assert!(cmd.apply(&mut world).is_ok());

        // Check ImpulseJoint on entity_a
        assert!(world.get::<ImpulseJoint>(entity_a).is_some());

        // Verify joint connects to entity_b, not a pin
        let joint = world.get::<ImpulseJoint>(entity_a).unwrap();
        assert_eq!(joint.parent, entity_b);

        // Check visual entity
        let children = world.get::<Children>(entity_a);
        assert!(children.is_some());
        let visual_id = *children
            .unwrap()
            .iter()
            .find(|&&child| world.get::<Connector>(child).is_some())
            .unwrap();

        // Check NO pin entity created
        assert!(cmd.pin_entity.is_none());

        // Undo
        cmd.undo(&mut world);

        // Check ImpulseJoint removed
        assert!(world.get::<ImpulseJoint>(entity_a).is_none());

        // Check visual entity despawned
        assert!(world.get_entity(visual_id).is_err());
    }
}
