//! Tool for creating rectangular rigid bodies.
//!
//! Click and drag to define the extents of a new box.

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

fn box_tool_update(
    mut commands: Commands,
    mut data: ResMut<BoxToolData>,
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
            // Spawn
            if size.x > 0.01 && size.y > 0.01 {
                let shape = shapes::Rectangle {
                    extents: Vec2::new(size.x as f32, size.y as f32),
                    origin: shapes::RectangleOrigin::Center,
                    ..default()
                };

                commands.spawn((
                    ShapeBuilder::with(&shape)
                        .fill(Color::srgb(0.5, 0.5, 1.0))
                        .stroke(Stroke::new(Color::BLACK, 0.1))
                        .build(),
                    RigidBody::Dynamic,
                    Collider::rectangle(size.x, size.y),
                    EditableBox {
                        width: size.x,
                        height: size.y,
                    },
                    Transform::from_xyz(center.x as f32, center.y as f32, 0.0),
                ));
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
