//! Tool for creating rectangular rigid bodies.
//!
//! Click and drag to define the extents of a new box.

use crate::commands::{CommandStack, GameCommand};
use crate::input::editable::EditableBox;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

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
    drag_start: Option<DVec2>,
}

fn box_tool_reset(mut data: ResMut<BoxToolData>) {
    data.drag_start = None;
}

fn calculate_box_geometry(start: DVec2, end: DVec2) -> (DVec2, DVec2) {
    let min = start.min(end);
    let max = start.max(end);
    let size = max - min;
    let center = min + size / 2.0;
    (size, center)
}

struct SpawnBoxCommand {
    width: f64,
    height: f64,
    x: f32,
    y: f32,
    entity: Option<Entity>,
}

impl SpawnBoxCommand {
    fn new(width: f64, height: f64, x: f32, y: f32) -> Self {
        Self {
            width,
            height,
            x,
            y,
            entity: None,
        }
    }
}

impl GameCommand for SpawnBoxCommand {
    fn execute(&mut self, world: &mut World) {
        let shape = shapes::Rectangle {
            extents: Vec2::new(self.width as f32, self.height as f32),
            origin: shapes::RectangleOrigin::Center,
            ..default()
        };

        let bundle = (
            ShapeBuilder::with(&shape)
                .fill(Color::srgb(0.5, 0.5, 1.0))
                .stroke(Stroke::new(Color::BLACK, 0.1))
                .build(),
            RigidBody::Dynamic,
            Collider::rectangle(self.width, self.height),
            EditableBox {
                width: self.width,
                height: self.height,
            },
            Transform::from_xyz(self.x, self.y, 0.0),
        );

        self.entity = Some(world.spawn(bundle).id());
    }

    fn undo(&mut self, world: &mut World) {
        if let Some(e) = self.entity {
            if world.get_entity(e).is_ok() {
                world.despawn(e);
            }
            self.entity = None;
        }
    }
}

fn box_tool_update(
    mut commands: Commands, // Kept for other things if needed, but we use CommandStack now
    mut data: ResMut<BoxToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut gizmos: Gizmos,
    mut contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
) {
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area() {
            return;
        }

    let Some(raw_pos) = cursor_pos.0 else {
        return;
    };

    let mut current_pos = raw_pos;
    if grid_settings.show && grid_settings.snap {
        let s = grid_settings.spacing;
        if s > 0.0001 {
            current_pos.x = (current_pos.x / s).round() * s;
            current_pos.y = (current_pos.y / s).round() * s;
        }
    }

    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start = Some(current_pos);
    }

    if let Some(start) = data.drag_start {
        let (size, center) = calculate_box_geometry(start, current_pos);

        if mouse.pressed(MouseButton::Left) {
            // Draw preview
            gizmos.rect_2d(
                Isometry2d::from_translation(Vec2::new(center.x as f32, center.y as f32)),
                Vec2::new(size.x as f32, size.y as f32),
                Color::WHITE,
            );
        }

        if mouse.just_released(MouseButton::Left) {
            if size.x > 0.01 && size.y > 0.01 {
                // Execute command via CommandStack
                // Note: CommandStack::push requires &mut World.
                // We are in a system, so we can't get &mut World directly.
                // We need to use `commands.add` to defer this action or run it as an exclusive system.
                // But CommandStack is a Resource.

                // Wait, CommandStack::push(&mut self, command, &mut World).
                // I can't call it from here because I don't have &mut World.

                // Solution: Make `CommandStack::push` NOT take `&mut World`, but `Commands`?
                // No, `execute` needs `&mut World`.

                // Standard Bevy pattern: Custom Command.
                // bevy::ecs::system::Command

                let cmd = SpawnBoxCommand::new(size.x, size.y, center.x as f32, center.y as f32);
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
    #[case(
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(5.0, 5.0)
    )]
    #[case(
        DVec2::new(10.0, 10.0),
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(5.0, 5.0)
    )]
    #[case(DVec2::new(-5.0, -5.0), DVec2::new(5.0, 5.0), DVec2::new(10.0, 10.0), DVec2::new(0.0, 0.0))]
    fn test_calculate_box_geometry(
        #[case] start: DVec2,
        #[case] end: DVec2,
        #[case] expected_size: DVec2,
        #[case] expected_center: DVec2,
    ) {
        let (size, center) = calculate_box_geometry(start, end);
        assert_eq!(size, expected_size);
        assert_eq!(center, expected_center);
    }
}
