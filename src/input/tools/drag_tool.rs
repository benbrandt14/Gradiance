//! Tool for dragging dynamic objects (the "Hand" tool).
//!
//! Allows the user to grab and move dynamic bodies using a mouse joint-like mechanic.
//! Currently implemented by calculating a target anchor and drawing lines.

use crate::input::tools::utils::calculate_local_anchor;
use crate::input::{PointerOverUi, ToolState, cursor::CursorWorldPos};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;

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
    local_anchor: Vec2,
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
    rapier_context_query: Query<&RapierContext>,
    mut query: Query<
        (&mut Transform, &Velocity),
        (With<RigidBody>, With<Collider>, Without<GroundPlane>),
    >,
    mut hand_query: Query<(&mut Transform, &mut Velocity), (With<RigidBody>, Without<Collider>)>,
    mut gizmos: Gizmos,
    pointer_over_ui: Res<PointerOverUi>,
    virtual_time: Res<Time<Virtual>>,
    time: Res<Time>,
) {
    if pointer_over_ui.0 && data.dragged_entity.is_none() {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };

        let filter = QueryFilter::default().exclude_sensors();
        let mut hit_entity: Option<Entity> = None;

        rapier_context.intersections_with_point(current_pos, filter, |entity| {
            hit_entity = Some(entity);
            false
        });

        if let Some(entity) = hit_entity
            && let Ok((transform, _)) = query.get(entity)
        {
            data.dragged_entity = Some(entity);

            // Calculate local anchor on the body
            data.local_anchor = calculate_local_anchor(transform, current_pos);

            // Spawn "Hand" kinematic body
            let hand = commands
                .spawn((
                    RigidBody::KinematicPositionBased,
                    Transform::from_xyz(current_pos.x, current_pos.y, 0.0),
                    Velocity::default(),
                ))
                .id();
            data.hand_entity = Some(hand);

            // Create RevoluteJoint (Mouse Joint) between Hand and Body
            let joint = RevoluteJointBuilder::new()
                .local_anchor1(Vec2::ZERO)
                .local_anchor2(data.local_anchor);

            commands
                .entity(hand)
                .insert(ImpulseJoint::new(entity, joint));
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
            let old_pos = t.translation.truncate();
            t.translation.x = current_pos.x;
            t.translation.y = current_pos.y;

            // Update velocity for correct physics interaction (kinematic body)
            v.linvel = calculate_drag_velocity(current_pos, old_pos, time.delta_secs());
            v.angvel = 0.0;
        }
    }

    // Handle dragging
    if let Some(entity) = data.dragged_entity {
        match query.get_mut(entity) {
            Ok((mut transform, _)) => {
                let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;

                // If paused, manually move the object to follow cursor.
                // If unpaused, we let the ImpulseJoint (Mouse Joint) handle the movement physically.
                if virtual_time.is_paused() {
                    let rotated_anchor = calculate_rotated_anchor(data.local_anchor, rotation);
                    let new_pos = current_pos - rotated_anchor;
                    transform.translation.x = new_pos.x;
                    transform.translation.y = new_pos.y;
                }

                // Draw line
                let rotated_anchor = calculate_rotated_anchor(data.local_anchor, rotation);
                let current_anchor_pos = transform.translation.truncate() + rotated_anchor;

                gizmos.line_2d(current_anchor_pos, current_pos, Color::WHITE);
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

fn calculate_rotated_anchor(local_anchor: Vec2, rotation: f32) -> Vec2 {
    let cos = rotation.cos();
    let sin = rotation.sin();
    Vec2::new(
        local_anchor.x * cos - local_anchor.y * sin,
        local_anchor.x * sin + local_anchor.y * cos,
    )
}

fn calculate_drag_velocity(current_pos: Vec2, old_pos: Vec2, delta_seconds: f32) -> Vec2 {
    if delta_seconds > 0.0001 {
        (current_pos - old_pos) / delta_seconds
    } else {
        Vec2::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::f32::consts::PI;

    #[rstest]
    #[case(Vec2::new(10.0, 0.0), Vec2::new(0.0, 0.0), 1.0, Vec2::new(10.0, 0.0))]
    #[case(Vec2::new(10.0, 0.0), Vec2::new(0.0, 0.0), 0.5, Vec2::new(20.0, 0.0))]
    #[case(Vec2::new(10.0, 0.0), Vec2::new(0.0, 0.0), 0.00001, Vec2::ZERO)] // Too small dt
    fn test_calculate_drag_velocity(
        #[case] current: Vec2,
        #[case] old: Vec2,
        #[case] dt: f32,
        #[case] expected: Vec2,
    ) {
        let v = calculate_drag_velocity(current, old, dt);
        assert!((v - expected).length() < 1e-5);
    }

    #[rstest]
    #[case(Vec2::new(1.0, 0.0), 0.0, Vec2::new(1.0, 0.0))]
    #[case(Vec2::new(1.0, 0.0), PI / 2.0, Vec2::new(0.0, 1.0))]
    #[case(Vec2::new(1.0, 0.0), PI, Vec2::new(-1.0, 0.0))]
    #[case(Vec2::new(0.0, 1.0), PI / 2.0, Vec2::new(-1.0, 0.0))]
    fn test_calculate_rotated_anchor(
        #[case] local: Vec2,
        #[case] rot: f32,
        #[case] expected: Vec2,
    ) {
        let result = calculate_rotated_anchor(local, rot);
        assert!((result.x - expected.x).abs() < 1e-5);
        assert!((result.y - expected.y).abs() < 1e-5);
    }
}
