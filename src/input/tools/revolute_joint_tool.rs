//! Tool for creating revolute joints (axles/hinges).
//!
//! Allows connecting two bodies with a pivot point, or pinning a body to the background.

use crate::input::tools::utils::{is_pointer_over_ui, calculate_local_anchor};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

/// Plugin for the Revolute Joint Tool.
pub struct RevoluteJointToolPlugin;

impl Plugin for RevoluteJointToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, revolute_joint_tool_update.run_if(in_state(ToolState::RevoluteJoint)));
    }
}

fn revolute_joint_tool_update(
    mut commands: Commands,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context: Res<RapierContext>,
    mut contexts: EguiContexts,
    transforms: Query<&Transform>,
) {
    if is_pointer_over_ui(&mut contexts) {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        // Find entities at cursor
        let point = Vec2::new(current_pos.x as f32, current_pos.y as f32);
        let mut intersections = Vec::new();
        rapier_context.intersections_with_point(point, QueryFilter::default(), |e| {
            intersections.push(e);
            true
        });

        if intersections.is_empty() {
            return;
        }

        if intersections.len() == 1 {
            let entity = intersections[0];
            // Connect entity to "World" (Static Pin)
            spawn_pin_joint(&mut commands, &transforms, entity, None, current_pos);
        } else {
            // Connect first two
            let entity_a = intersections[0];
            let entity_b = intersections[1];
            spawn_pin_joint(&mut commands, &transforms, entity_a, Some(entity_b), current_pos);
        }
    }
}

fn spawn_pin_joint(
    commands: &mut Commands,
    transforms: &Query<&Transform>,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_world: DVec2,
) {
    // Helper to get local anchor
    let get_local = |e: Entity| -> Vec2 {
        if let Ok(t) = transforms.get(e) {
            let local = calculate_local_anchor(t, anchor_world);
            Vec2::new(local.x as f32, local.y as f32)
        } else {
            Vec2::ZERO
        }
    };

    let anchor_a = get_local(entity_a);

    // Pin visual construction helper
    let spawn_visuals = |commands: &mut Commands, parent: Entity| {
        let v1 = commands
            .spawn((
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shapes::Circle {
                        radius: 5.0,
                        ..default()
                    }),
                    ..default()
                },
                Fill::color(Color::BLACK),
                Transform::from_translation(Vec3::Z * 0.1),
                GlobalTransform::default(),
                VisibilityBundle::default(),
            ))
            .id();

        let v2 = commands
            .spawn((
                ShapeBundle {
                    path: GeometryBuilder::build_as(&shapes::Circle {
                        radius: 2.0,
                        ..default()
                    }),
                    ..default()
                },
                Fill::color(Color::WHITE),
                Transform::from_translation(Vec3::Z * 0.2),
                GlobalTransform::default(),
                VisibilityBundle::default(),
            ))
            .id();

        commands.entity(parent).add_child(v1).add_child(v2);
    };

    if let Some(entity_b) = entity_b {
        let anchor_b = get_local(entity_b);

        let axle = commands
            .spawn((
                RigidBody::Dynamic,
                AdditionalMassProperties::Mass(0.01),
                Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 10.0),
                GlobalTransform::default(),
                VisibilityBundle::default(),
                Collider::ball(0.5),
                Sensor,
            ))
            .id();

        spawn_visuals(commands, axle);

        commands.entity(axle).insert(
            ImpulseJoint::new(entity_a, FixedJointBuilder::new().local_anchor1(Vec2::ZERO).local_anchor2(anchor_a))
        );

        commands.entity(entity_b).insert(
            ImpulseJoint::new(axle, RevoluteJointBuilder::new().local_anchor1(anchor_b).local_anchor2(Vec2::ZERO))
        );

    } else {
        // Connect to World (Static Body)
        let pin = commands
            .spawn((
                RigidBody::Fixed,
                Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 0.0),
                Collider::ball(0.5),
                Sensor,
                GlobalTransform::default(),
                VisibilityBundle::default(),
            ))
            .id();

        spawn_visuals(commands, pin);

        commands.entity(entity_a).insert(
            ImpulseJoint::new(pin, RevoluteJointBuilder::new().local_anchor1(anchor_a).local_anchor2(Vec2::ZERO))
        );
    }
}
