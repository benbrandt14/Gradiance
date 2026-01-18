//! Tool for creating revolute joints (axles/hinges).
//!
//! Allows connecting two bodies with a pivot point, or pinning a body to the background.

use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use avian2d::prelude::*;
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
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    transforms: Query<&Transform>,
) {
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area() {
            return;
        }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        // Find entities at cursor
        let filter = SpatialQueryFilter::default();
        let intersections = spatial_query.point_intersections(current_pos, &filter);

        if intersections.is_empty() {
            return;
        }

        let point = current_pos;

        if intersections.len() == 1 {
            let entity = intersections[0];
            // Connect entity to "World" (Static Pin)
            spawn_pin_joint(&mut commands, &transforms, entity, None, point);
        } else {
            // Connect first two
            let entity_a = intersections[0];
            let entity_b = intersections[1];
            spawn_pin_joint(&mut commands, &transforms, entity_a, Some(entity_b), point);
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
    let get_local = |e: Entity| -> DVec2 {
        if let Ok(t) = transforms.get(e) {
            let rotation = t.rotation.to_euler(EulerRot::XYZ).2 as f64;
            let translation = t.translation.truncate().as_dvec2();
            let relative = anchor_world - translation;
            let cos = rotation.cos();
            let sin = rotation.sin();
            // Rotate back
            DVec2::new(
                relative.x * cos + relative.y * sin,
                -relative.x * sin + relative.y * cos,
            )
        } else {
            DVec2::ZERO
        }
    };

    let anchor_a = get_local(entity_a);

    // Pin visual construction helper
    // Returns a bundle or spawns children to an entity
    let spawn_visuals = |commands: &mut Commands, parent: Entity| {
        let v1 = commands
            .spawn((
                ShapeBuilder::with(&shapes::Circle {
                    radius: 5.0,
                    ..default()
                })
                .fill(Color::BLACK)
                .build(),
                Transform::from_translation(Vec3::Z * 0.1),
            ))
            .id();

        let v2 = commands
            .spawn((
                ShapeBuilder::with(&shapes::Circle {
                    radius: 2.0,
                    ..default()
                })
                .fill(Color::WHITE)
                .build(),
                Transform::from_translation(Vec3::Z * 0.2),
            ))
            .id();

        commands.entity(parent).add_children(&[v1, v2]);
    };

    if let Some(entity_b) = entity_b {
        let anchor_b = get_local(entity_b);
        let joint_entity = commands
            .spawn((
                RevoluteJoint::new(entity_a, entity_b)
                    .with_local_anchor1(anchor_a)
                    .with_local_anchor2(anchor_b)
                    .with_point_compliance(1e-9),
                Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 10.0),
                GlobalTransform::default(),
                Visibility::default(),
                InheritedVisibility::default(),
                ViewVisibility::default(),
            ))
            .id();

        spawn_visuals(commands, joint_entity);
    } else {
        // Connect to World (Static Body)
        // Spawn a static body at anchor_world
        let pin = commands
            .spawn((
                RigidBody::Static,
                Collider::circle(0.1),
                Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 0.0),
            ))
            .id();

        spawn_visuals(commands, pin);

        let anchor_b = DVec2::ZERO;

        commands.spawn((
            RevoluteJoint::new(entity_a, pin)
                .with_local_anchor1(anchor_a)
                .with_local_anchor2(anchor_b)
                .with_point_compliance(1e-9),
        ));
    }
}
