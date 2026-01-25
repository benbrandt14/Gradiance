//! Tool for creating polygon rigid bodies.
//!
//! Click to place vertices, and click near the start point to close the loop and spawn the polygon.
//! Uses Convex Hull decomposition for colliders.

use crate::input::commands::{CommandStack, SpawnPolygonCommand};
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy_egui::EguiContexts;

/// Action returned by the polygon tool input logic.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PolygonToolAction {
    /// No specific action triggered.
    None,
    /// Add a new point to the polygon.
    AddPoint(Vec2),
    /// Close the loop and create the polygon.
    CloseLoop,
}

/// Result of processing polygon tool input.
#[derive(Debug, PartialEq)]
pub struct PolygonInputResult {
    /// The current cursor position (potentially snapped).
    pub snapped_cursor: Vec2,
    /// The action determined by the input.
    pub action: PolygonToolAction,
}

/// Pure logic for handling polygon tool input.
#[allow(clippy::too_many_arguments)]
pub fn handle_polygon_input_logic(
    cursor_pos: Option<Vec2>,
    mouse_just_pressed: bool,
    enter_just_pressed: bool,
    _grid_show: bool,
    _grid_snap: bool,
    _grid_spacing: f32,
    points: &[Vec2],
) -> Option<PolygonInputResult> {
    let raw_pos = cursor_pos?;

    let current_pos = raw_pos;

    let mut action = PolygonToolAction::None;

    if mouse_just_pressed {
        if !points.is_empty() {
            let start = points[0];
            if should_close_loop(start, current_pos) {
                action = PolygonToolAction::CloseLoop;
            } else {
                action = PolygonToolAction::AddPoint(current_pos);
            }
        } else {
            action = PolygonToolAction::AddPoint(current_pos);
        }
    } else if enter_just_pressed {
        action = PolygonToolAction::CloseLoop;
    }

    Some(PolygonInputResult {
        snapped_cursor: current_pos,
        action,
    })
}

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

fn should_close_loop(start: Vec2, current: Vec2) -> bool {
    start.distance(current) < 0.5
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
    if is_pointer_over_ui(&mut contexts) {
        return;
    }

    // Use the pure logic function
    let Some(result) = handle_polygon_input_logic(
        cursor_pos.0,
        mouse.just_pressed(MouseButton::Left),
        keys.just_pressed(KeyCode::Enter),
        grid_settings.show,
        grid_settings.snap_to_grid,
        grid_settings.spacing,
        &data.points,
    ) else {
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
            if data.points.len() >= 3 {
                // Close loop and spawn
                let center = data.points.iter().fold(Vec2::ZERO, |acc, p| acc + *p)
                    / data.points.len() as f32;

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
    #[case(Vec2::ZERO, Vec2::new(0.4, 0.0), true)]
    #[case(Vec2::ZERO, Vec2::new(0.5, 0.0), false)]
    #[case(Vec2::ZERO, Vec2::new(0.6, 0.0), false)]
    fn test_should_close_loop(#[case] start: Vec2, #[case] current: Vec2, #[case] expected: bool) {
        assert_eq!(should_close_loop(start, current), expected);
    }

    #[rstest]
    #[case(
        Some(Vec2::new(10.0, 10.0)), // cursor
        true, false, // mouse, enter
        false, false, 1.0, // grid
        vec![], // points
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(10.0, 10.0),
            action: PolygonToolAction::AddPoint(Vec2::new(10.0, 10.0))
        })
    )]
    #[case(
        Some(Vec2::new(10.0, 10.0)),
        false, false, // no input
        false, false, 1.0,
        vec![],
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(10.0, 10.0),
            action: PolygonToolAction::None
        })
    )]
    #[case(
        Some(Vec2::new(0.1, 0.1)), // near start (0,0)
        true, false,
        false, false, 1.0,
        vec![Vec2::ZERO, Vec2::new(10.0, 0.0)], // existing points
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(0.1, 0.1),
            action: PolygonToolAction::CloseLoop // Should close
        })
    )]
    #[case(
        Some(Vec2::new(10.0, 10.0)),
        false, true, // enter pressed
        false, false, 1.0,
        vec![Vec2::ZERO, Vec2::new(10.0, 0.0)],
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(10.0, 10.0),
            action: PolygonToolAction::CloseLoop
        })
    )]
    #[case(
        Some(Vec2::new(10.2, 10.8)),
        true, false,
        true, true, 1.0, // Snap enabled (but ignored in function now)
        vec![],
        Some(PolygonInputResult {
            snapped_cursor: Vec2::new(10.2, 10.8), // Not Snapped by function
            action: PolygonToolAction::AddPoint(Vec2::new(10.2, 10.8))
        })
    )]
    fn test_handle_polygon_input_logic(
        #[case] cursor: Option<Vec2>,
        #[case] mouse: bool,
        #[case] enter: bool,
        #[case] grid_show: bool,
        #[case] grid_snap: bool,
        #[case] spacing: f32,
        #[case] points: Vec<Vec2>,
        #[case] expected: Option<PolygonInputResult>,
    ) {
        let result = handle_polygon_input_logic(
            cursor, mouse, enter, grid_show, grid_snap, spacing, &points,
        );
        assert_eq!(result, expected);
    }
}
