//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::input::{ToolState, cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::DVec2;
use bevy_egui::EguiContexts;

/// Plugin for the Select Tool.
pub struct SelectToolPlugin;

impl Plugin for SelectToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectToolData>();
        app.add_systems(
            Update,
            select_tool_update.run_if(in_state(ToolState::Select)),
        );
        app.add_systems(OnExit(ToolState::Select), select_tool_reset);
    }
}

#[derive(Resource, Default)]
struct SelectToolData {
    drag_start: Option<DVec2>,
    is_moving: bool,
    // Store offsets for multiple entities
    // Map Entity -> Initial DVec2
    initial_positions: Vec<(Entity, DVec2)>,
    drag_start_pos: DVec2,
}

fn select_tool_reset(mut data: ResMut<SelectToolData>) {
    *data = SelectToolData::default();
}

fn select_tool_update(
    mut selection: ResMut<Selection>,
    mut data: ResMut<SelectToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    mut gizmos: Gizmos,
    mut query: Query<&mut Transform>,
    grid_settings: Res<GridSettings>,
) {
    // Prevent selection if over UI
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area() && !data.is_moving && data.drag_start.is_none() {
            return;
        }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start_pos = current_pos;

        let filter = SpatialQueryFilter::default();
        if let Some(hit) = spatial_query.project_point(current_pos, true, &filter) {
            // Clicked on something

            // If shift not held and entity not in selection, clear selection
            if !shift && !selection.0.contains(&hit.entity) {
                selection.clear();
            }

            if shift {
                selection.toggle(hit.entity);
            } else if !selection.0.contains(&hit.entity) {
                selection.add(hit.entity);
            }

            // Initiate Move for all selected
            if selection.0.contains(&hit.entity) {
                data.is_moving = true;
                data.initial_positions.clear();
                for &entity in &selection.0 {
                    if let Ok(t) = query.get(entity) {
                        data.initial_positions.push((entity, t.translation.truncate().as_dvec2()));
                    }
                }
            }
        } else {
            // Clicked on empty space -> Box Select
            if !shift {
                selection.clear();
            }
            data.is_moving = false;
            data.drag_start = Some(current_pos);
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if data.is_moving {
            // Move the entities
            let delta = current_pos - data.drag_start_pos;

            for (entity, initial_pos) in &data.initial_positions {
                 if let Ok(mut t) = query.get_mut(*entity) {
                    let mut new_pos = *initial_pos + delta;
                    if grid_settings.show && grid_settings.snap {
                        new_pos = snap_to_grid(new_pos, grid_settings.spacing);
                    }
                    t.translation.x = new_pos.x as f32;
                    t.translation.y = new_pos.y as f32;
                 }
            }

        } else if let Some(start) = data.drag_start {
            // Draw Box
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;
            let center = (min + max) / 2.0;

            gizmos.rect_2d(
                Isometry2d::from_translation(Vec2::new(center.x as f32, center.y as f32)),
                Vec2::new(size.x as f32, size.y as f32),
                Color::srgb(0.0, 1.0, 1.0),
            );
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if !data.is_moving
            && let Some(start) = data.drag_start {
                // Box Select Finalize
                let min = start.min(current_pos);
                let max = start.max(current_pos);
                let size = max - min;

                if size.x > 0.1 && size.y > 0.1 {
                    let center = (min + max) / 2.0;
                    // Avian 0.5.0: Collider::rectangle takes width, height
                    let shape = Collider::rectangle(size.x, size.y);
                    let position = center;
                    let rotation = 0.0;
                    let filter = SpatialQueryFilter::default();
                    let hits =
                        spatial_query.shape_intersections(&shape, position, rotation, &filter);

                    for entity in hits {
                        selection.add(entity);
                    }
                }
            }

        data.is_moving = false;
        data.drag_start = None;
        data.initial_positions.clear();
    }
}
