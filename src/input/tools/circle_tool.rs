use crate::input::editable::EditableCircle;
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

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
    drag_start: Option<DVec2>,
}

fn circle_tool_reset(mut data: ResMut<CircleToolData>) {
    data.drag_start = None;
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
    if let Ok(ctx) = contexts.ctx_mut() {
        if ctx.is_pointer_over_area() {
            return;
        }
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
        let radius = start.distance(current_pos);

        if mouse.pressed(MouseButton::Left) {
            gizmos.circle_2d(
                Isometry2d::from_translation(Vec2::new(start.x as f32, start.y as f32)),
                radius as f32,
                Color::WHITE,
            );
            gizmos.line_2d(
                Vec2::new(start.x as f32, start.y as f32),
                Vec2::new(current_pos.x as f32, current_pos.y as f32),
                Color::WHITE,
            );
        }

        if mouse.just_released(MouseButton::Left) {
            if radius > 0.01 {
                let shape = shapes::Circle {
                    radius: radius as f32,
                    center: Vec2::ZERO,
                };

                commands.spawn((
                    ShapeBuilder::with(&shape)
                        .fill(Color::srgb(1.0, 0.5, 0.5))
                        .stroke(Stroke::new(Color::BLACK, 0.1))
                        .build(),
                    RigidBody::Dynamic,
                    Collider::circle(radius),
                    EditableCircle { radius },
                    Transform::from_xyz(start.x as f32, start.y as f32, 0.0),
                ));
            }

            data.drag_start = None;
        }
    }
}
