//! Tool for dragging dynamic objects (the "Hand" tool).
//!
//! Allows the user to grab and move dynamic bodies using a mouse joint-like mechanic.
//! Currently implemented by calculating a target anchor and drawing lines, but physics force
//! application is temporarily disabled pending `ExternalForce` integration.

use crate::input::tools::utils::{is_pointer_over_ui, calculate_local_anchor};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use avian2d::prelude::*;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;

/// Plugin for the Drag Tool.
pub struct DragToolPlugin;

impl Plugin for DragToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragToolData>();
        app.add_systems(Update, drag_tool_update.run_if(in_state(ToolState::Drag)));
        app.add_systems(OnExit(ToolState::Drag), drag_tool_reset);
    }
}

#[derive(Resource, Default)]
struct DragToolData {
    dragged_entity: Option<Entity>,
    hand_entity: Option<Entity>,
    local_anchor: DVec2,
}

fn drag_tool_reset(mut commands: Commands, mut data: ResMut<DragToolData>) {
    if let Some(hand) = data.hand_entity {
        commands.entity(hand).despawn();
    }
    data.dragged_entity = None;
    data.hand_entity = None;
}

fn drag_tool_update(
    mut commands: Commands,
    mut data: ResMut<DragToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial_query: SpatialQuery,
    mut query: Query<(&mut Transform, &LinearVelocity, &AngularVelocity), (With<RigidBody>, With<Collider>)>,
    mut hand_query: Query<(&mut Transform, &mut LinearVelocity), (With<RigidBody>, Without<Collider>)>,
    mut gizmos: Gizmos,
    mut contexts: EguiContexts,
    virtual_time: Res<Time<Virtual>>,
    time: Res<Time>,
) {
    if is_pointer_over_ui(&mut contexts) && data.dragged_entity.is_none() {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let filter = SpatialQueryFilter::default();
        if let Some(hit) = spatial_query.project_point(current_pos, true, &filter)
            && let Ok((transform, _, _)) = query.get(hit.entity)
        {
            data.dragged_entity = Some(hit.entity);

            // Calculate local anchor on the body
            data.local_anchor = calculate_local_anchor(transform, current_pos);

            // Spawn "Hand" kinematic body
            let hand = commands.spawn((
                RigidBody::Kinematic,
                Transform::from_xyz(current_pos.x as f32, current_pos.y as f32, 0.0),
            )).id();
            data.hand_entity = Some(hand);

            // Create RevoluteJoint (Mouse Joint) between Hand and Body
            commands.spawn((
                RevoluteJoint::new(hand, hit.entity)
                    .with_local_anchor1(DVec2::ZERO)
                    .with_local_anchor2(data.local_anchor)
                    // High compliance = springy, Low = rigid.
                    // We want it to be somewhat springy to avoid instability, but stiff enough to drag.
                    // 0.00001 is fairly stiff.
                    .with_point_compliance(1e-5),
            ));
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if let Some(hand) = data.hand_entity {
            commands.entity(hand).despawn();
        }
        data.hand_entity = None;
        data.dragged_entity = None;
    }

    if let Some(hand) = data.hand_entity {
        // Move hand to cursor
        if let Ok((mut t, mut v)) = hand_query.get_mut(hand) {
            // Update transform for visual/logic consistency
            let old_pos = t.translation.truncate().as_dvec2();
            t.translation.x = current_pos.x as f32;
            t.translation.y = current_pos.y as f32;

            // Update velocity for correct physics interaction (kinematic body)
            if time.delta_secs() > 0.0001 {
                let velocity = (current_pos - old_pos) / time.delta_secs_f64();
                v.x = velocity.x;
                v.y = velocity.y;
            } else {
                v.x = 0.0;
                v.y = 0.0;
            }
        }
    }

    // Handle dragging
    if let Some(entity) = data.dragged_entity {
        match query.get_mut(entity) {
            Ok((mut transform, _, _)) => {
                let rotation = transform.rotation.to_euler(EulerRot::XYZ).2 as f64;

                // If paused, manually move the object to follow cursor
                if virtual_time.is_paused() {
                    let cos = rotation.cos();
                    let sin = rotation.sin();
                    let rotated_anchor = DVec2::new(
                        data.local_anchor.x * cos - data.local_anchor.y * sin,
                        data.local_anchor.x * sin + data.local_anchor.y * cos,
                    );

                    let new_pos = current_pos - rotated_anchor;
                    transform.translation.x = new_pos.x as f32;
                    transform.translation.y = new_pos.y as f32;
                }

                // Draw line
                let cos = rotation.cos();
                let sin = rotation.sin();
                let rotated_anchor = DVec2::new(
                    data.local_anchor.x * cos - data.local_anchor.y * sin,
                    data.local_anchor.x * sin + data.local_anchor.y * cos,
                );
                let current_anchor_pos = transform.translation.truncate().as_dvec2() + rotated_anchor;

                gizmos.line_2d(
                    Vec2::new(current_anchor_pos.x as f32, current_anchor_pos.y as f32),
                    Vec2::new(current_pos.x as f32, current_pos.y as f32),
                    Color::WHITE,
                );
            }
            Err(_) => {
                // Entity lost. Cleanup.
                if let Some(hand) = data.hand_entity {
                    commands.entity(hand).despawn();
                }
                data.hand_entity = None;
                data.dragged_entity = None;
            }
        }
    }
}
