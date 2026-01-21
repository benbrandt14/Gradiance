//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos, selection::Selection};
use crate::physics::floor::GroundPlane;
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
            select_tool_update.run_if(in_state(ToolState::Select)),
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
    mut query: Query<(Entity, &mut Transform, Option<&GroundPlane>)>,
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

        // Collect all hits
        let mut hits = Vec::new();
        rapier_context.intersections_with_point(current_pos, filter, |entity| {
            hits.push(entity);
            true
        });

        // Sort hits: Non-ground first, then by Z index (descending)
        hits.sort_by(|&a, &b| {
            let info_a = query.get(a);
            let info_b = query.get(b);

            if let (Ok((_, t_a, g_a)), Ok((_, t_b, g_b))) = (info_a, info_b) {
                // Non-ground (None) > Ground (Some)
                // We want None < Some (asc) so None comes first?
                // is_some(): None=false, Some=true. false < true.
                // So None comes first. Correct.
                let ground_order = g_a.is_some().cmp(&g_b.is_some());
                if ground_order != std::cmp::Ordering::Equal {
                     return ground_order;
                }

                // Z index: Higher is better (comes first).
                // Sort descending: b.cmp(a)
                t_b.translation.z.partial_cmp(&t_a.translation.z).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                std::cmp::Ordering::Equal
            }
        });

        if let Some(&entity) = hits.first() {
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
                    if let Ok((_, t, _)) = query.get(entity) {
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
                if let Ok((_, mut t, _)) = query.get_mut(*entity) {
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
                let mut count = 0;
                // Manual AABB check against all selectable entities
                for (entity, global_transform) in &selectable_query {
                    let t = global_transform.translation().truncate();
                    if is_point_in_box(t, min, max) {
                        // Insert directly to avoid spamming "Added entity" logs
                        if selection.0.insert(entity) {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    info!("Select Tool: Box Selected {} entities", count);
                }
            }
        }

        if data.is_moving {
            info!("Select Tool: Moved {} entities", selection.0.len());
        }

        data.is_moving = false;
        data.drag_start = None;
        data.initial_positions.clear();
    }
}

fn is_point_in_box(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(5.0, 5.0), true)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(-5.0, 5.0), false)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(15.0, 5.0), false)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(5.0, 15.0), false)]
    fn test_box_selection_logic(
        #[case] start: Vec2,
        #[case] end: Vec2,
        #[case] point: Vec2,
        #[case] expected: bool,
    ) {
        let min = start.min(end);
        let max = start.max(end);
        let contained = is_point_in_box(point, min, max);
        assert_eq!(contained, expected);
    }
}
