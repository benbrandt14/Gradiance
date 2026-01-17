//! Tool for creating polygon rigid bodies.
//!
//! Click to place vertices, and click near the start point to close the loop and spawn the polygon.
//! Uses Convex Hull decomposition for colliders.

use crate::input::{ToolState, cursor::CursorWorldPos};

use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;

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
    points: Vec<DVec2>,
}

fn polygon_tool_reset(mut data: ResMut<PolygonToolData>) {
    data.points.clear();
}

fn polygon_tool_update(
    mut commands: Commands,
    mut data: ResMut<PolygonToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut gizmos: Gizmos,
    mut contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
) {
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.is_pointer_over_area()
    {
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
                Vec2::new(p1.x as f32, p1.y as f32),
                Vec2::new(p2.x as f32, p2.y as f32),
                Color::WHITE,
            );
        }
        // Line to cursor
        let last = data.points.last().unwrap();
        gizmos.line_2d(
            Vec2::new(last.x as f32, last.y as f32),
            Vec2::new(current_pos.x as f32, current_pos.y as f32),
            Color::WHITE,
        );

        // Draw start point marker
        let start = data.points[0];
        gizmos.circle_2d(
            Isometry2d::from_translation(Vec2::new(start.x as f32, start.y as f32)),
            0.2,                        // snap radius visual
            Color::srgb(0.0, 1.0, 0.0), // Green
        );
    }

    if mouse.just_pressed(MouseButton::Left) {
        // Check if closing loop
        if !data.points.is_empty() {
            let start = data.points[0];
            // Allow closing loop even if snapped, but check distance to (snapped) start
            if start.distance(current_pos) < 0.5 {
                // Snap distance
                if data.points.len() >= 3 {
                    // Close loop and spawn
                    let center = data.points.iter().fold(DVec2::ZERO, |acc, p| acc + *p)
                        / data.points.len() as f64;

                    // Points relative to center
                    let relative_points: Vec<DVec2> =
                        data.points.iter().map(|p| *p - center).collect();
                    let vec2_points: Vec<Vec2> = relative_points
                        .iter()
                        .map(|p| Vec2::new(p.x as f32, p.y as f32))
                        .collect();

                    // Create shape
                    let shape = shapes::Polygon {
                        points: vec2_points.clone(),
                        closed: true,
                    };

                    commands.spawn((
                        ShapeBuilder::with(&shape)
                            .fill(Color::srgb(0.5, 1.0, 0.5))
                            .stroke(Stroke::new(Color::BLACK, 0.1))
                            .build(),
                        RigidBody::Dynamic,
                        Collider::convex_hull(relative_points).unwrap_or(Collider::circle(1.0)), // Fallback
                        Transform::from_xyz(center.x as f32, center.y as f32, 0.0),
                    ));

                    data.points.clear();
                }
                return;
            }
        }

        data.points.push(current_pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn calculate_center(points: &[DVec2]) -> DVec2 {
        points.iter().fold(DVec2::ZERO, |acc, p| acc + *p) / points.len() as f64
    }

    #[rstest]
    fn test_calculate_center() {
        let points = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(0.0, 10.0),
        ];
        let center = calculate_center(&points);
        assert_eq!(center, DVec2::new(5.0, 5.0));
    }
}
