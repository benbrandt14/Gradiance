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

fn should_close_loop(start: Vec2, current: Vec2) -> bool {
    start.distance(current) < 0.5
}

/// Action to be taken by the system based on logic result.
#[derive(Debug, PartialEq)]
pub enum PolygonToolAction {
    /// No action needed.
    None,
    /// Update the preview visual.
    UpdatePreview {
        /// The current cursor position (potentially snapped).
        current_pos: Vec2,
    },
    /// Spawn the polygon.
    Spawn {
        /// The center position of the polygon.
        position: Vec2,
        /// The vertices relative to the center.
        vertices: Vec<Vec2>,
    },
}

/// Pure logic for polygon tool input handling.
pub fn handle_polygon_input_logic(
    cursor_pos: Option<Vec2>,
    mouse_just_pressed: bool,
    enter_pressed: bool,
    is_pointer_over_ui: bool,
    grid_show: bool,
    grid_snap: bool,
    grid_spacing: f32,
    points: &mut Vec<Vec2>,
) -> PolygonToolAction {
    if is_pointer_over_ui {
        return PolygonToolAction::None;
    }

    let Some(raw_pos) = cursor_pos else {
        return PolygonToolAction::None;
    };

    let mut current_pos = raw_pos;
    if grid_show && grid_snap {
        current_pos = snap_to_grid(current_pos, grid_spacing);
    }

    let mut should_close = false;

    if mouse_just_pressed {
        if !points.is_empty() {
            let start = points[0];
            if should_close_loop(start, current_pos) {
                should_close = true;
            } else {
                points.push(current_pos);
            }
        } else {
            points.push(current_pos);
        }
    }

    if enter_pressed {
        should_close = true;
    }

    if should_close && points.len() >= 3 {
        // Calculate center and relative vertices
        let center = points.iter().fold(Vec2::ZERO, |acc, p| acc + *p) / points.len() as f32;
        let relative_points: Vec<Vec2> = points.iter().map(|p| *p - center).collect();

        let action = PolygonToolAction::Spawn {
            position: center,
            vertices: relative_points,
        };
        points.clear();
        return action;
    }

    PolygonToolAction::UpdatePreview { current_pos }
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
    let action = handle_polygon_input_logic(
        cursor_pos.0,
        mouse.just_pressed(MouseButton::Left),
        keys.just_pressed(KeyCode::Enter),
        is_pointer_over_ui(&mut contexts),
        grid_settings.show,
        grid_settings.snap,
        grid_settings.spacing,
        &mut data.points,
    );

    match action {
        PolygonToolAction::None => {}
        PolygonToolAction::UpdatePreview { current_pos } => {
            if !data.points.is_empty() {
                for i in 0..data.points.len() - 1 {
                    let p1 = data.points[i];
                    let p2 = data.points[i + 1];
                    gizmos.line_2d(p1, p2, Color::srgb(1.0, 1.0, 1.0));
                }
                // Line to cursor
                let last = data.points.last().unwrap();
                gizmos.line_2d(*last, current_pos, Color::srgb(1.0, 1.0, 1.0));

                // Draw start point marker
                let start = data.points[0];
                gizmos.circle_2d(
                    Isometry2d::from_translation(Vec2::new(start.x, start.y)),
                    0.5,                        // snap radius visual
                    Color::srgb(0.0, 1.0, 0.0), // Green
                );
            }
        }
        PolygonToolAction::Spawn { position, vertices } => {
            let cmd = SpawnPolygonCommand {
                position,
                vertices,
                entity: None,
            };

            commands.queue(move |world: &mut World| {
                world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                    stack.push(Box::new(cmd), world);
                });
            });
        }
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
    #[case::basic_input(
        Some(Vec2::new(10.0, 10.0)), // cursor
        false, false, false, // mouse, enter, ui
        false, false, 1.0, // grid
        vec![], // points
        PolygonToolAction::UpdatePreview { current_pos: Vec2::new(10.0, 10.0) },
        vec![] // points unchanged
    )]
    #[case::add_point(
        Some(Vec2::new(10.0, 10.0)),
        true, false, false,
        false, false, 1.0,
        vec![],
        PolygonToolAction::UpdatePreview { current_pos: Vec2::new(10.0, 10.0) },
        vec![Vec2::new(10.0, 10.0)]
    )]
    #[case::add_second_point(
        Some(Vec2::new(20.0, 20.0)),
        true, false, false,
        false, false, 1.0,
        vec![Vec2::new(10.0, 10.0)],
        PolygonToolAction::UpdatePreview { current_pos: Vec2::new(20.0, 20.0) },
        vec![Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)]
    )]
    #[case::close_loop_click(
        Some(Vec2::new(10.2, 10.2)), // Near start (10,10)
        true, false, false,
        false, false, 1.0,
        vec![Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0), Vec2::new(20.0, 20.0)],
        PolygonToolAction::Spawn {
            position: Vec2::new(50.0 / 3.0, 40.0 / 3.0),
            vertices: vec![
                Vec2::new(10.0, 10.0) - Vec2::new(50.0 / 3.0, 40.0 / 3.0),
                Vec2::new(20.0, 10.0) - Vec2::new(50.0 / 3.0, 40.0 / 3.0),
                Vec2::new(20.0, 20.0) - Vec2::new(50.0 / 3.0, 40.0 / 3.0)
            ]
        },
        vec![] // Cleared
    )]
    #[case::close_loop_enter(
        Some(Vec2::new(30.0, 30.0)), // Position doesn't matter for enter
        false, true, false,
        false, false, 1.0,
        vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)],
        PolygonToolAction::Spawn {
            position: Vec2::new(10.0/3.0, 10.0/3.0),
            vertices: vec![
                Vec2::new(0.0, 0.0) - Vec2::new(10.0/3.0, 10.0/3.0),
                Vec2::new(10.0, 0.0) - Vec2::new(10.0/3.0, 10.0/3.0),
                Vec2::new(0.0, 10.0) - Vec2::new(10.0/3.0, 10.0/3.0)
            ]
        },
        vec![] // Cleared
    )]
    #[case::grid_snap(
        Some(Vec2::new(10.2, 10.8)),
        true, false, false,
        true, true, 1.0,
        vec![],
        PolygonToolAction::UpdatePreview { current_pos: Vec2::new(10.0, 11.0) },
        vec![Vec2::new(10.0, 11.0)]
    )]
    #[case::ui_block(
        Some(Vec2::new(10.0, 10.0)),
        true, false, true, // UI is true
        false, false, 1.0,
        vec![],
        PolygonToolAction::None,
        vec![]
    )]
    #[case::no_cursor(
        None,
        true, false, false,
        false, false, 1.0,
        vec![],
        PolygonToolAction::None,
        vec![]
    )]
    #[case::enter_too_few_points(
        Some(Vec2::new(30.0, 30.0)),
        false, true, false,
        false, false, 1.0,
        vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)], // Only 2 points
        PolygonToolAction::UpdatePreview { current_pos: Vec2::new(30.0, 30.0) },
        vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)] // Not cleared, not spawned
    )]
    fn test_handle_polygon_input_logic(
        #[case] cursor_pos: Option<Vec2>,
        #[case] mouse_just_pressed: bool,
        #[case] enter_pressed: bool,
        #[case] is_pointer_over_ui: bool,
        #[case] grid_show: bool,
        #[case] grid_snap: bool,
        #[case] grid_spacing: f32,
        #[case] initial_points: Vec<Vec2>,
        #[case] expected_action: PolygonToolAction,
        #[case] expected_points: Vec<Vec2>,
    ) {
        let mut points = initial_points;
        let action = handle_polygon_input_logic(
            cursor_pos,
            mouse_just_pressed,
            enter_pressed,
            is_pointer_over_ui,
            grid_show,
            grid_snap,
            grid_spacing,
            &mut points,
        );

        assert_eq!(action, expected_action);
        assert_eq!(points, expected_points);
    }
}
