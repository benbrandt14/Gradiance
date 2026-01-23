//! Tool for creating circular rigid bodies.
//!
//! Click and drag to define the radius of a new circle.

use crate::input::commands::{CommandStack, SpawnCircleCommand};
use crate::input::tools::utils::{DragStatus, handle_drag_input};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
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
    contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
) {
    let Some(drag) = handle_drag_input(
        cursor_pos,
        mouse,
        grid_settings,
        contexts,
        &mut data.drag_start,
    ) else {
        return;
    };

    let radius = calculate_radius(drag.start, drag.current);

    match drag.status {
        DragStatus::Dragging => {
            gizmos.circle_2d(
                Isometry2d::from_translation(Vec2::new(drag.start.x, drag.start.y)),
                radius,
                Color::WHITE,
            );
            gizmos.line_2d(drag.start, drag.current, Color::WHITE);
        }
        DragStatus::Finished => {
            if should_spawn_circle(radius) {
                let cmd = SpawnCircleCommand {
                    position: drag.start,
                    radius,
                    entity: None,
                };

                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }
        }
        _ => {}
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
