//! Tool for creating rectangular rigid bodies.
//!
//! Click and drag to define the extents of a new box.

use crate::input::commands::{CommandStack, SpawnBoxCommand};
use crate::input::tools::utils::{DragStatus, handle_drag_input};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy_egui::EguiContexts;

/// Plugin for the Box Tool.
pub struct BoxToolPlugin;

impl Plugin for BoxToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BoxToolData>();
        app.add_systems(Update, box_tool_update.run_if(in_state(ToolState::Box)));
        app.add_systems(OnExit(ToolState::Box), box_tool_reset);
    }
}

#[derive(Resource, Default)]
struct BoxToolData {
    drag_start: Option<Vec2>,
}

fn box_tool_reset(mut data: ResMut<BoxToolData>) {
    data.drag_start = None;
}

fn calculate_box_geometry(start: Vec2, end: Vec2) -> (Vec2, Vec2) {
    let min = start.min(end);
    let max = start.max(end);
    let size = max - min;
    let center = min + size / 2.0;
    (size, center)
}

fn should_spawn_box(size: Vec2) -> bool {
    size.x > 0.01 && size.y > 0.01
}

fn box_tool_update(
    mut commands: Commands,
    mut data: ResMut<BoxToolData>,
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

    let (size, center) = calculate_box_geometry(drag.start, drag.current);

    match drag.status {
        DragStatus::Dragging => {
            // Draw preview
            gizmos.rect_2d(
                Isometry2d::from_translation(Vec2::new(center.x, center.y)),
                Vec2::new(size.x, size.y),
                Color::WHITE,
            );
        }
        DragStatus::Finished => {
            if should_spawn_box(size) {
                let cmd = SpawnBoxCommand::new(center, size.x, size.y);

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
    #[case(
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(5.0, 5.0)
    )]
    #[case(
        Vec2::new(10.0, 10.0),
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
        Vec2::new(5.0, 5.0)
    )]
    #[case(Vec2::new(-5.0, -5.0), Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 0.0))]
    fn test_calculate_box_geometry(
        #[case] start: Vec2,
        #[case] end: Vec2,
        #[case] expected_size: Vec2,
        #[case] expected_center: Vec2,
    ) {
        let (size, center) = calculate_box_geometry(start, end);
        assert_eq!(size, expected_size);
        assert_eq!(center, expected_center);
    }

    #[rstest]
    #[case(Vec2::new(1.0, 1.0), true)]
    #[case(Vec2::new(0.02, 0.02), true)]
    #[case(Vec2::new(0.01, 0.01), false)]
    #[case(Vec2::new(0.009, 0.009), false)]
    #[case(Vec2::new(1.0, 0.009), false)]
    #[case(Vec2::new(0.009, 1.0), false)]
    fn test_should_spawn_box(#[case] size: Vec2, #[case] expected: bool) {
        assert_eq!(should_spawn_box(size), expected);
    }
}
