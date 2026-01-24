//! Tool for creating polygon rigid bodies.
//!
//! Click to place vertices, and click near the start point to close the loop and spawn the polygon.
//! Uses Convex Hull decomposition for colliders.

use crate::input::commands::{CommandStack, SpawnPolygonCommand};
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy_egui::EguiContexts;

/// Plugin for the Polygon Tool.
pub struct PolygonToolPlugin;

impl Plugin for PolygonToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PolygonToolData>();
        app.add_systems(
            Update,
            polygon_tool_update.run_if(in_state(ToolState::Polygon)),
        );
        app.add_systems(OnExit(ToolState::Polygon), polygon_tool_reset);
    }
}

#[derive(Resource, Default)]
struct PolygonToolData {
    points: Vec<Vec2>,
}

fn polygon_tool_reset(mut data: ResMut<PolygonToolData>) {
    data.points.clear();
}

/// Action to be taken by the tool system.
#[derive(Debug, Clone, PartialEq)]
pub enum PolygonToolAction {
    /// Do nothing.
    None,
    /// Add a new point to the polygon.
    AddPoint(Vec2),
    /// Close the loop and spawn the polygon.
    CloseLoop,
}

/// Result of processing input.
#[derive(Debug, Clone, PartialEq)]
pub struct PolygonInputResult {
    /// The cursor position to use for visual previews (snapped if grid is on).
    pub snapped_cursor: Vec2,
    /// The action to perform.
    pub action: PolygonToolAction,
}

/// Pure logic for polygon tool input.
pub fn handle_polygon_input_logic(
    cursor_pos: Option<Vec2>,
    mouse_just_pressed: bool,
    enter_pressed: bool,
    is_over_ui: bool,
    grid_snap: bool,
    grid_spacing: f32,
    points: &[Vec2],
) -> Option<PolygonInputResult> {
    if is_over_ui {
        return None;
    }

    let raw_pos = cursor_pos?;
    let mut current_pos = raw_pos;

    if grid_snap {
        current_pos = snap_to_grid(current_pos, grid_spacing);
    }

    let mut action = PolygonToolAction::None;

    if enter_pressed {
         // Only close if we have enough points
         if points.len() >= 3 {
             action = PolygonToolAction::CloseLoop;
         }
    } else if mouse_just_pressed {
        // Check if closing loop
        let should_close = if let Some(start) = points.first() {
            // Allow closing loop even if snapped, but check distance to (snapped) start
            start.distance(current_pos) < 0.5
        } else {
            false
        };

        if should_close {
             if points.len() >= 3 {
                 action = PolygonToolAction::CloseLoop;
             }
        } else {
            action = PolygonToolAction::AddPoint(current_pos);
        }
    }

    Some(PolygonInputResult {
        snapped_cursor: current_pos,
        action,
    })
}

fn polygon_tool_update(
    mut commands: Commands,
    mut data: ResMut<PolygonToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut gizmos: Gizmos,
    mut contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
) {
    let logic_result = handle_polygon_input_logic(
        cursor_pos.0,
        mouse.just_pressed(MouseButton::Left),
        keys.just_pressed(KeyCode::Enter),
        is_pointer_over_ui(&mut contexts),
        grid_settings.show && grid_settings.snap,
        grid_settings.spacing,
        &data.points,
    );

    let Some(result) = logic_result else {
        return;
    };

    let current_pos = result.snapped_cursor;

    // Draw preview lines
    if !data.points.is_empty() {
        for i in 0..data.points.len() - 1 {
            let p1 = data.points[i];
            let p2 = data.points[i + 1];
            gizmos.line_2d(p1, p2, Color::WHITE);
        }
        // Line to cursor
        let last = data.points.last().unwrap();
        gizmos.line_2d(*last, current_pos, Color::WHITE);

        // Draw start point marker
        let start = data.points[0];
        gizmos.circle_2d(
            Isometry2d::from_translation(Vec2::new(start.x, start.y)),
            0.5,                        // snap radius visual (increased)
            Color::srgb(0.0, 1.0, 0.0), // Green
        );
    }

    match result.action {
        PolygonToolAction::AddPoint(p) => {
            data.points.push(p);
        }
        PolygonToolAction::CloseLoop => {
             let center =
                data.points.iter().fold(Vec2::ZERO, |acc, p| acc + *p) / data.points.len() as f32;

            // Points relative to center
            let relative_points: Vec<Vec2> = data.points.iter().map(|p| *p - center).collect();

            let cmd = SpawnPolygonCommand {
                position: center,
                vertices: relative_points,
                entity: None,
            };

            commands.queue(move |world: &mut World| {
                world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                    stack.push(Box::new(cmd), world);
                });
            });

            data.points.clear();
        }
        PolygonToolAction::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn calculate_center(points: &[Vec2]) -> Vec2 {
        points.iter().fold(Vec2::ZERO, |acc, p| acc + *p) / points.len() as f32
    }

    #[rstest]
    fn test_calculate_center() {
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let center = calculate_center(&points);
        assert_eq!(center, Vec2::new(5.0, 5.0));
    }

    #[rstest]
    #[case::basic_add(
        Some(Vec2::ZERO), true, false, false, false, 1.0, &[],
        Some(PolygonInputResult {
            snapped_cursor: Vec2::ZERO,
            action: PolygonToolAction::AddPoint(Vec2::ZERO)
        })
    )]
    #[case::over_ui(
        Some(Vec2::ZERO), true, false, true, false, 1.0, &[],
        None
    )]
    #[case::grid_snap(
        Some(Vec2::new(0.9, 0.9)), true, false, false, true, 1.0, &[],
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(1.0, 1.0),
            action: PolygonToolAction::AddPoint(Vec2::new(1.0, 1.0))
        })
    )]
    #[case::close_loop_click(
        Some(Vec2::ZERO), true, false, false, false, 1.0,
        &[Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)], // 3 pts, clicking start
        Some(PolygonInputResult {
            snapped_cursor: Vec2::ZERO,
            action: PolygonToolAction::CloseLoop
        })
    )]
    #[case::not_close_loop_not_enough_pts(
        Some(Vec2::ZERO), true, false, false, false, 1.0,
        &[Vec2::ZERO, Vec2::new(1.0, 0.0)], // 2 pts, clicking start
        Some(PolygonInputResult {
            snapped_cursor: Vec2::ZERO,
            action: PolygonToolAction::None
        })
    )]
    #[case::close_loop_enter(
        Some(Vec2::new(5.0, 5.0)), false, true, false, false, 1.0,
        &[Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(1.0, 1.0)], // 3 pts, enter
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(5.0, 5.0), // cursor pos irrelevant for action but returned
            action: PolygonToolAction::CloseLoop
        })
    )]
    fn test_handle_polygon_input_logic(
        #[case] cursor_pos: Option<Vec2>,
        #[case] mouse: bool,
        #[case] enter: bool,
        #[case] ui: bool,
        #[case] snap: bool,
        #[case] spacing: f32,
        #[case] points: &[Vec2],
        #[case] expected: Option<PolygonInputResult>,
    ) {
        let result = handle_polygon_input_logic(
            cursor_pos,
            mouse,
            enter,
            ui,
            snap,
            spacing,
            points
        );
        assert_eq!(result, expected);
    }
}
