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
    _commands: Commands,
    mut data: ResMut<DragToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial_query: SpatialQuery,
    mut query: Query<(&Transform, &mut LinearVelocity, &mut AngularVelocity), With<RigidBody>>,
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
            && let Ok((transform, _, _)) = query.get(hit.entity)
        {
            data.dragged_entity = Some(hit.entity);

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
        if let Ok((transform, mut lin_vel, mut ang_vel)) = query.get_mut(entity) {
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

            // Kinematic Velocity Control (Fallback for ExternalForce)
            let delta = current_pos - current_anchor_pos;
            lin_vel.0 = delta * 15.0;
            ang_vel.0 *= 0.95;

        } else {
            // Entity doesn't exist anymore or logic failed
            data.dragged_entity = None;
        }
    }
}
