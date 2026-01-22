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
        app.add_systems(Update, ground_tool_update.run_if(in_state(ToolState::Ground)));
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
            gizmos.line_2d(drag.start, drag.current, Color::srgb(1.0, 1.0, 1.0));
            // Draw normal to show "down" direction (where the ground body will be)
            let mid = (drag.start + drag.current) / 2.0;
            let dir = drag.current - drag.start;
            // Normal rotated -90 degrees (x, y) -> (y, -x)
            let normal = Vec2::new(dir.y, -dir.x).normalize_or_zero() * 20.0;
            gizmos.line_2d(mid, mid + normal, Color::srgb(0.5, 0.5, 0.5));
        }
        DragStatus::Finished => {
            let start = drag.start;
            let current = drag.current;

            // Calculate vector
            let mut diff = current - start;
            // If dragging is too short, default to horizontal (length 1.0 to right)
            if diff.length_squared() < 0.1 {
                diff = Vec2::new(1.0, 0.0);
            }

            let rotation = diff.to_angle();
            let center = (start + current) / 2.0;

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
