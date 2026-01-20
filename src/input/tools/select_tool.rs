//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::physics::floor::GroundPlane;
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos, selection::Selection};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy_egui::EguiContexts;

/// Plugin for the Select Tool.
pub struct SelectToolPlugin;

impl Plugin for SelectToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectToolData>();
        app.add_systems(
            Update,
            select_tool_update
                .run_if(in_state(ToolState::Select))
                .in_set(GameSystemSet::Tools),
        );
        app.add_systems(OnExit(ToolState::Select), select_tool_reset);
    }
}

#[derive(Resource, Default)]
struct SelectToolData {
    drag_start: Option<Vec2>,
    is_moving: bool,
    // Store offsets for multiple entities
    // Map Entity -> Initial Vec2
    initial_positions: Vec<(Entity, Vec2)>,
    drag_start_pos: Vec2,
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
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
    mut gizmos: Gizmos,
    mut query: Query<&mut Transform>,
    // Add query for box selection fallback
    selectable_query: Query<(Entity, &GlobalTransform), (With<Collider>, Without<GroundPlane>)>,
    grid_settings: Res<GridSettings>,
) {
    // Prevent selection if over UI
    if is_pointer_over_ui(&mut contexts) && !data.is_moving && data.drag_start.is_none() {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start_pos = current_pos;

        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };
        let filter = QueryFilter::default().exclude_sensors();
        let mut hit_entity: Option<Entity> = None;

        rapier_context.intersections_with_point(current_pos, filter, |entity| {
            hit_entity = Some(entity);
            false
        });

        if let Some(entity) = hit_entity {
            // Clicked on something

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
                for &entity in &selection.0 {
                    if let Ok(t) = query.get(entity) {
                        data.initial_positions
                            .push((entity, t.translation.truncate()));
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
                    t.translation.x = new_pos.x;
                    t.translation.y = new_pos.y;
                }
            }
        } else if let Some(start) = data.drag_start {
            // Draw Box
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;
            let center = (min + max) / 2.0;

            gizmos.rect_2d(
                Isometry2d::from_translation(Vec2::new(center.x, center.y)),
                Vec2::new(size.x, size.y),
                Color::srgb(0.0, 1.0, 1.0),
            );
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if !data.is_moving
            && let Some(start) = data.drag_start
        {
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
                    let t = global_transform.translation().truncate();
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
