//! Tool for creating infinite ground planes.
//!
//! Click and drag to define the surface and rotation of the ground.

use crate::input::commands::{CommandStack, SpawnGroundCommand};
use crate::input::tools::utils::{DragStatus, handle_drag_input};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy_egui::EguiContexts;

/// Plugin for the Ground Tool.
pub struct GroundToolPlugin;

impl Plugin for GroundToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GroundToolData>();
        app.add_systems(
            Update,
            ground_tool_update.run_if(in_state(ToolState::Ground)),
        );
        app.add_systems(OnExit(ToolState::Ground), ground_tool_reset);
    }
}

#[derive(Resource, Default)]
struct GroundToolData {
    drag_start: Option<Vec2>,
}

fn ground_tool_reset(mut data: ResMut<GroundToolData>) {
    data.drag_start = None;
}

fn calculate_ground_geometry(start: Vec2, current: Vec2) -> (Vec2, f32) {
    let center = (start + current) / 2.0;
    let mut diff = current - start;
    // If dragging is too short, default to horizontal (length 1.0 to right)
    if diff.length_squared() < 0.1 {
        diff = Vec2::new(1.0, 0.0);
    }
    let rotation = diff.to_angle();
    (center, rotation)
}

fn ground_tool_update(
    mut commands: Commands,
    mut data: ResMut<GroundToolData>,
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

    match drag.status {
        DragStatus::Dragging => {
            // Draw preview line
            gizmos.line_2d(drag.start, drag.current, Color::WHITE);
            // Draw normal to show "down" direction (where the ground body will be)
            let mid = (drag.start + drag.current) / 2.0;
            let dir = drag.current - drag.start;
            // Normal rotated -90 degrees (x, y) -> (y, -x)
            let normal = Vec2::new(dir.y, -dir.x).normalize_or_zero() * 20.0;
            gizmos.line_2d(mid, mid + normal, Color::srgb(0.5, 0.5, 0.5));
        }
        DragStatus::Finished => {
            let (center, rotation) = calculate_ground_geometry(drag.start, drag.current);

            let cmd = SpawnGroundCommand {
                position: center,
                rotation,
                entity: None,
            };

            commands.queue(move |world: &mut World| {
                world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                    stack.push(Box::new(cmd), world);
                });
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::f32::consts::PI;

    #[rstest]
    #[case(Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(1.0, 0.0), 0.0)]
    #[case(Vec2::ZERO, Vec2::new(0.0, 2.0), Vec2::new(0.0, 1.0), PI / 2.0)]
    #[case(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::ZERO, 0.0)]
    fn test_calculate_ground_geometry(
        #[case] start: Vec2,
        #[case] current: Vec2,
        #[case] expected_center: Vec2,
        #[case] expected_rotation: f32,
    ) {
        let (center, rotation) = calculate_ground_geometry(start, current);
        assert_eq!(center, expected_center);
        assert!((rotation - expected_rotation).abs() < 1e-5);
    }

    #[rstest]
    fn test_calculate_ground_geometry_short_drag() {
        let start = Vec2::ZERO;
        let current = Vec2::new(0.01, 0.0); // Very short drag
        let (center, rotation) = calculate_ground_geometry(start, current);

        // Center should still be midpoint
        assert_eq!(center, Vec2::new(0.005, 0.0));
        // Rotation should default to 0.0 (horizontal) because drag was too short
        assert_eq!(rotation, 0.0);
    }
}
