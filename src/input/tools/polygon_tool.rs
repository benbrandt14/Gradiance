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

    let Some(raw_pos) = cursor_pos.0 else {
        return;
    };

    let mut current_pos = raw_pos;
    if grid_settings.show && grid_settings.snap {
        current_pos = snap_to_grid(current_pos, grid_settings.spacing);
    }

    // Draw preview lines
    if !data.points.is_empty() {
        for i in 0..data.points.len() - 1 {
            let p1 = data.points[i];
            let p2 = data.points[i + 1];
            gizmos.line_2d(
                p1,
                p2,
                Color::WHITE,
            );
        }
        // Line to cursor
        let last = data.points.last().unwrap();
        gizmos.line_2d(
            *last,
            current_pos,
            Color::WHITE,
        );

        // Draw start point marker
        let start = data.points[0];
        gizmos.circle_2d(
            Isometry2d::from_translation(Vec2::new(start.x, start.y)),
            0.5,                        // snap radius visual (increased)
            Color::srgb(0.0, 1.0, 0.0), // Green
        );
    }

    let mut should_close = false;

    if mouse.just_pressed(MouseButton::Left) {
        // Check if closing loop
        if !data.points.is_empty() {
            let start = data.points[0];
            // Allow closing loop even if snapped, but check distance to (snapped) start
            if should_close_loop(start, current_pos) {
                should_close = true;
            } else {
                data.points.push(current_pos);
            }
        } else {
            data.points.push(current_pos);
        }
    }

    if keys.just_pressed(KeyCode::Enter) {
        should_close = true;
    }

    if should_close
        && data.points.len() >= 3 {
            // Close loop and spawn
            let center =
                data.points.iter().fold(Vec2::ZERO, |acc, p| acc + *p) / data.points.len() as f32;

            // Points relative to center
            let relative_points: Vec<Vec2> = data.points
                .iter()
                .map(|p| (*p - center))
                .collect();

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
}
