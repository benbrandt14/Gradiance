//! Tool for creating circular rigid bodies.
//!
//! Click and drag to define the radius of a new circle.

use crate::input::commands::{CommandStack, SpawnCircleCommand};
use crate::input::tools::utils::is_pointer_over_ui;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy_egui::EguiContexts;

/// Plugin for the Circle Tool.
pub struct CircleToolPlugin;

impl Plugin for CircleToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CircleToolData>();
        app.add_systems(
            Update,
            circle_tool_update.run_if(in_state(ToolState::Circle)),
        );
        app.add_systems(OnExit(ToolState::Circle), circle_tool_reset);
    }
}

#[derive(Resource, Default)]
struct CircleToolData {
    drag_start: Option<Vec2>,
}

fn circle_tool_reset(mut data: ResMut<CircleToolData>) {
    data.drag_start = None;
}

fn calculate_radius(start: Vec2, end: Vec2) -> f32 {
    start.distance(end)
}

fn should_spawn_circle(radius: f32) -> bool {
    radius > 0.01
}

fn circle_tool_update(
    mut commands: Commands,
    mut data: ResMut<CircleToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
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

    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start = Some(current_pos);
    }

    if let Some(start) = data.drag_start {
        let radius = calculate_radius(start, current_pos);

        if mouse.pressed(MouseButton::Left) {
            gizmos.circle_2d(
                Isometry2d::from_translation(Vec2::new(start.x, start.y)),
                radius,
                Color::WHITE,
            );
            gizmos.line_2d(start, current_pos, Color::WHITE);
        }

        if mouse.just_released(MouseButton::Left) {
            if should_spawn_circle(radius) {
                let cmd = SpawnCircleCommand {
                    position: start,
                    radius,
                    entity: None,
                };

                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }

            data.drag_start = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Vec2::ZERO, Vec2::new(1.0, 0.0), 1.0)]
    #[case(Vec2::ZERO, Vec2::new(0.0, 2.0), 2.0)]
    #[case(Vec2::new(1.0, 1.0), Vec2::new(4.0, 5.0), 5.0)] // 3-4-5 triangle
    fn test_calculate_radius(#[case] start: Vec2, #[case] end: Vec2, #[case] expected: f32) {
        let radius = calculate_radius(start, end);
        assert!((radius - expected).abs() < 1e-6);
    }

    #[rstest]
    #[case(1.0, true)]
    #[case(0.02, true)]
    #[case(0.01, false)] // > 0.01
    #[case(0.009, false)]
    #[case(0.0, false)]
    fn test_should_spawn_circle(#[case] radius: f32, #[case] expected: bool) {
        assert_eq!(should_spawn_circle(radius), expected);
    }
}
