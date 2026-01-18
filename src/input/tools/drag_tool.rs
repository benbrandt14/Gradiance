//! Tool for dragging dynamic objects (the "Hand" tool).
//!
//! Allows the user to grab and move dynamic bodies using a mouse joint-like mechanic.
//! Currently implemented by calculating a target anchor and drawing lines, but physics force
//! application is temporarily disabled pending `ExternalForce` integration.

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
    local_anchor: DVec2,
}

fn drag_tool_reset(mut data: ResMut<DragToolData>) {
    data.dragged_entity = None;
}

fn drag_tool_update(
    mut commands: Commands,
    mut data: ResMut<DragToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial_query: SpatialQuery,
    mut query: Query<
        (
            &Transform,
            &LinearVelocity,
            &AngularVelocity,
            Option<&mut ExternalForce>,
            Option<&CenterOfMass>,
        ),
        With<RigidBody>,
    >,
    mut gizmos: Gizmos,
    mut contexts: EguiContexts,
) {
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area()
        && data.dragged_entity.is_none()
    {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let filter = SpatialQueryFilter::default();
        if let Some(hit) = spatial_query.project_point(current_pos, true, &filter)
            && let Ok((transform, _, _, force, _)) = query.get(hit.entity)
        {
            data.dragged_entity = Some(hit.entity);

            // Ensure ExternalForce exists
            if force.is_none() {
                commands
                    .entity(hit.entity)
                    .insert(ExternalForce::default().with_persistence(false));
            }

            // Calculate local anchor
            // inverse transform
            let rotation = transform.rotation.to_euler(EulerRot::XYZ).2 as f64;
            let translation = transform.translation.truncate().as_dvec2();
            let relative = current_pos - translation;

            let cos = rotation.cos();
            let sin = rotation.sin();
            // Rotate back: x' = x cos + y sin, y' = -x sin + y cos
            data.local_anchor = DVec2::new(
                relative.x * cos + relative.y * sin,
                -relative.x * sin + relative.y * cos,
            );
        }
    }

    if mouse.just_released(MouseButton::Left) {
        data.dragged_entity = None;
    }

    if let Some(entity) = data.dragged_entity {
        if let Ok((transform, lin_vel, ang_vel, force_opt, com)) = query.get_mut(entity) {
            // Draw gizmo line regardless of physics application (feedback for user)
            let rotation = transform.rotation.to_euler(EulerRot::XYZ).2 as f64;
            let translation = transform.translation.truncate().as_dvec2();

            let cos = rotation.cos();
            let sin = rotation.sin();
            // Rotate forward: x' = x cos - y sin, y' = x sin + y cos
            let rotated_anchor = DVec2::new(
                data.local_anchor.x * cos - data.local_anchor.y * sin,
                data.local_anchor.x * sin + data.local_anchor.y * cos,
            );

            let current_anchor_pos = translation + rotated_anchor;

            gizmos.line_2d(
                Vec2::new(current_anchor_pos.x as f32, current_anchor_pos.y as f32),
                Vec2::new(current_pos.x as f32, current_pos.y as f32),
                Color::WHITE,
            );

            if let Some(mut force) = force_opt {
                // Force persistence needs to be false for this control method, or we need to reset it every frame.
                // If we added it with persistence(false), it clears itself.
                // But if it was already there (and maybe true), we might be accumulating.
                // Safer to set it to zero first if we are controlling it.
                force.clear();

                let delta = current_pos - current_anchor_pos;

                // PD Controller parameters
                let stiffness = 200.0;
                let damping = 10.0;

                // Calculate velocity of the anchor point
                // V_point = V_com + w x r
                // Note: lin_vel is velocity of Center of Mass (usually).
                // rotated_anchor is relative to Transform origin.
                // If CoM != Transform origin, we need to adjust.
                // But usually for simple shapes they are close.
                // Let's assume Transform origin ~ CoM for now or that rotated_anchor is roughly correct relative to CoM.
                // If we have CoM:
                let com_offset = if let Some(com) = com {
                    com.0
                } else {
                    DVec2::ZERO
                };
                // The anchor vector relative to CoM is (rotated_anchor_relative_to_transform_origin - com_offset_rotated?)
                // Actually CoM component is local offset from Transform.
                // So rotated_anchor (relative to Transform) - rotated_com = anchor relative to CoM.

                // For simplicity, let's stick to using linear velocity of the body and approximating point velocity.
                // V_p = V_b + w x r_from_b
                let r = rotated_anchor; // Vector from Transform origin to anchor.
                let point_velocity = lin_vel.0 + DVec2::new(-ang_vel.0 * r.y, ang_vel.0 * r.x);

                let force_vector = delta * stiffness - point_velocity * damping;

                // Apply force at point
                // ExternalForce::apply_force_at_point args: (force, point, center_of_mass)
                // point: Point where force is applied relative to body origin (Transform).
                // center_of_mass: Center of mass relative to body origin.

                let com_rotated = DVec2::new(
                    com_offset.x * cos - com_offset.y * sin,
                    com_offset.x * sin + com_offset.y * cos,
                );

                force.apply_force_at_point(force_vector, rotated_anchor, com_rotated);
            }
        } else {
            // Entity doesn't exist anymore or logic failed
            data.dragged_entity = None;
        }
    }
}
