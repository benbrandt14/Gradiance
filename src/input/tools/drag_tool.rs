//! Tool for dragging dynamic objects (the "Hand" tool).
//!
//! Allows the user to grab and move dynamic bodies using a mouse joint-like mechanic.
//! Separates input logic from physics implementation.

use crate::events::{CommitDragEvent, DragEntitiesEvent};
use crate::input::tools::utils::calculate_local_anchor;
use crate::input::{PointerOverUi, ToolState, cursor::CursorWorldPos};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;

/// Plugin for the Drag Tool.
pub struct DragToolPlugin;

impl Plugin for DragToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DragToolData>();
        app.add_systems(
            Update,
            (
                drag_tool_input,
                drag_tool_physics,
            )
                .run_if(in_state(ToolState::Drag)),
        );
        app.add_systems(OnExit(ToolState::Drag), drag_tool_reset);
    }
}

#[derive(Resource, Default)]
struct DragToolData {
    /// The entity currently being dragged.
    dragged_entity: Option<Entity>,
    /// The visual/physics "hand" entity.
    hand_entity: Option<Entity>,
    /// Local anchor point on the dragged entity.
    local_anchor: Vec2,
    /// Current target position for the hand (cursor pos).
    target_pos: Vec2,

    // For Undo
    start_pos: Vec2,
    start_rot: f32,
}

fn drag_tool_reset(mut commands: Commands, mut data: ResMut<DragToolData>) {
    if let Some(hand) = data.hand_entity {
        commands.entity(hand).despawn();
    }
    data.dragged_entity = None;
    data.hand_entity = None;
}

/// Handles input, updates `DragToolData`, and emits events for Paused dragging / Undo.
#[allow(clippy::too_many_arguments)]
fn drag_tool_input(
    mut data: ResMut<DragToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context_query: Query<&RapierContext>,
    query: Query<
        (Entity, &Transform),
        (With<RigidBody>, With<Collider>, Without<GroundPlane>),
    >,
    pointer_over_ui: Res<PointerOverUi>,
    virtual_time: Res<Time<Virtual>>,
    mut ev_drag: EventWriter<DragEntitiesEvent>,
    mut ev_commit: EventWriter<CommitDragEvent>,
) {
    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    // Update target pos continuously for physics system
    data.target_pos = current_pos;

    if pointer_over_ui.0 && data.dragged_entity.is_none() {
        return;
    }

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
            && let Ok((_, transform)) = query.get(entity)
        {
            data.dragged_entity = Some(entity);
            data.local_anchor = calculate_local_anchor(transform, current_pos);
            data.start_pos = transform.translation.truncate();
            data.start_rot = transform.rotation.to_euler(EulerRot::XYZ).2;
        }
    }

    // Drag Update (Paused Mode)
    if data.dragged_entity.is_some() && virtual_time.is_paused() {
        if let Some(entity) = data.dragged_entity {
            if let Ok((_, transform)) = query.get(entity) {
                 let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
                 let rotated_anchor = calculate_rotated_anchor(data.local_anchor, rotation);
                 let new_pos = current_pos - rotated_anchor;

                 ev_drag.send(DragEntitiesEvent {
                     positions: vec![(entity, new_pos)],
                     rotations: Vec::new(),
                 });
            }
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if let Some(entity) = data.dragged_entity {
            // Commit Undo
            // Calculate final state
            if let Ok((_, transform)) = query.get(entity) {
                let end_pos = transform.translation.truncate();
                let end_rot = transform.rotation;

                // Only commit if changed
                if (end_pos - data.start_pos).length_squared() > 0.001
                   || (end_rot.to_euler(EulerRot::XYZ).2 - data.start_rot).abs() > 0.001
                {
                     ev_commit.send(CommitDragEvent {
                         position_changes: vec![(entity, data.start_pos, end_pos)],
                         rotation_changes: vec![(entity, Quat::from_rotation_z(data.start_rot), end_rot)],
                     });
                }
            }
        }
        data.dragged_entity = None;
    }
}

/// Handles physics of dragging (Spawning hand, moving hand).
fn drag_tool_physics(
    mut commands: Commands,
    mut data: ResMut<DragToolData>,
    mut hand_query: Query<(&mut Transform, &mut Velocity), (With<RigidBody>, Without<Collider>)>,
    mut gizmos: Gizmos,
    query: Query<&Transform, With<Collider>>, // To get current pos of dragged entity for line drawing
    time: Res<Time>,
) {
    // Check state transitions
    if data.dragged_entity.is_some() && data.hand_entity.is_none() {
        // Start Drag: Spawn Hand
        let hand = commands
            .spawn((
                RigidBody::KinematicPositionBased,
                Transform::from_xyz(data.target_pos.x, data.target_pos.y, 0.0),
                Velocity::default(),
            ))
            .id();
        data.hand_entity = Some(hand);

        // Create Joint
        let joint = RevoluteJointBuilder::new()
            .local_anchor1(Vec2::ZERO)
            .local_anchor2(data.local_anchor);

        commands
            .entity(hand)
            .insert(ImpulseJoint::new(data.dragged_entity.unwrap(), joint));
    } else if data.dragged_entity.is_none() && data.hand_entity.is_some() {
        // Stop Drag: Despawn Hand
        commands.entity(data.hand_entity.unwrap()).despawn();
        data.hand_entity = None;
    }

    // Update Hand
    if let Some(hand) = data.hand_entity {
        let target = data.target_pos;

        if let Ok((mut t, mut v)) = hand_query.get_mut(hand) {
            let old_pos = t.translation.truncate();
            t.translation.x = target.x;
            t.translation.y = target.y;

            v.linvel = calculate_drag_velocity(target, old_pos, time.delta_secs());
            v.angvel = 0.0;
        }

        // Draw gizmo line
        if let Some(dragged) = data.dragged_entity {
            if let Ok(t) = query.get(dragged) {
                 let rotation = t.rotation.to_euler(EulerRot::XYZ).2;
                 let rotated_anchor = calculate_rotated_anchor(data.local_anchor, rotation);
                 let anchor_world = t.translation.truncate() + rotated_anchor;
                 gizmos.line_2d(anchor_world, target, Color::WHITE);
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
