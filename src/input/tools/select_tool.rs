//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::input::{ToolState, cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use crate::GroundPlane;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_picking::prelude::*;

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
    mut pointer_events: EventReader<Pointer<Down>>,
    mut contexts: EguiContexts,
    mut gizmos: Gizmos,
    mut query: Query<&mut Transform>,
    // Add query for box selection fallback
    selectable_query: Query<(Entity, &GlobalTransform), (With<Collider>, Without<GroundPlane>)>,
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

    let mut clicked_entity = false;

    // Handle picking events
    for event in pointer_events.read() {
        if event.button == PointerButton::Primary {
            clicked_entity = true;
            data.drag_start_pos = current_pos;
            let entity = event.target;

            // If shift not held and entity not in selection, clear selection
            if !shift && !selection.0.contains(&entity) {
                selection.clear();
            }

            if shift {
                selection.toggle(entity);
            } else if !selection.0.contains(&entity) {
                selection.add(entity);
            }

            // Initiate Move for all selected
            if selection.0.contains(&entity) {
                data.is_moving = true;
                data.initial_positions.clear();
                for &e in &selection.0 {
                    if let Ok(t) = query.get(e) {
                        data.initial_positions.push((e, t.translation.truncate().as_dvec2()));
                    }
                }
            }
        }
    }

    if mouse.just_pressed(MouseButton::Left) && !clicked_entity {
        // Clicked on empty space -> Box Select
        if !shift {
            selection.clear();
        }
        data.is_moving = false;
        data.drag_start = Some(current_pos);
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
                    let min_x = min.x;
                    let max_x = max.x;
                    let min_y = min.y;
                    let max_y = max.y;

                    // Manual AABB check against all selectable entities
                    for (entity, global_transform) in &selectable_query {
                        let t = global_transform.translation().truncate().as_dvec2();
                        if t.x >= min_x && t.x <= max_x && t.y >= min_y && t.y <= max_y {
                            selection.add(entity);
                        }
                    }
                }
            }

        data.is_moving = false;
        data.drag_start = None;
        data.initial_positions.clear();
    }
}
