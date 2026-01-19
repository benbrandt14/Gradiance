//! Infinite grid system.
//!
//! Visualizes a grid on the background and provides snapping utility functions.

use crate::prelude::*;
use bevy::window::PrimaryWindow;

/// Plugin for rendering the grid and handling grid settings.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GridSettings>();
        app.add_systems(
            Update,
            draw_grid.run_if(|settings: Res<GridSettings>| settings.show),
        );
    }
}

/// Enum for different grid types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Reflect, Default)]
pub enum GridType {
    /// Standard rectangular grid.
    #[default]
    Rectangular,
    /// Isometric grid (30 degrees).
    Isometric,
}

/// Settings for grid visibility and snapping.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct GridSettings {
    /// Whether to render the grid lines.
    pub show: bool,
    /// Whether tools should snap positions to the grid.
    pub snap: bool,
    /// The distance between grid lines (in world units).
    pub spacing: f32,
    /// The type of grid to display.
    pub grid_type: GridType,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            show: true,
            snap: true,
            spacing: 1.0,
            grid_type: GridType::Rectangular,
        }
    }
}

/// Snaps a position to the nearest grid intersection based on the given spacing.
///
/// # Arguments
///
/// * `pos` - The raw world position.
/// * `spacing` - The grid spacing interval.
///
/// # Returns
///
/// The snapped position.
pub fn snap_to_grid(pos: Vec2, spacing: f32) -> Vec2 {
    if spacing <= 0.0001 {
        return pos;
    }
    Vec2::new(
        (pos.x / spacing).round() * spacing,
        (pos.y / spacing).round() * spacing,
    )
}

fn draw_grid(
    mut settings: ResMut<GridSettings>,
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<Camera2d>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mut gizmos: Gizmos,
) {
    let Some((camera, transform, projection)) = camera_query.iter().next() else {
        return;
    };

    let Some(window) = window_query.iter().next() else {
        return;
    };

    // Calculate visible area using viewport_to_world_2d
    // We get the world position of the top-left and bottom-right corners of the screen.
    let top_left_screen = Vec2::ZERO;
    let bottom_right_screen = Vec2::new(window.width(), window.height());

    let Ok(top_left) = camera.viewport_to_world_2d(transform, top_left_screen) else {
        return;
    };
    let Ok(bottom_right) = camera.viewport_to_world_2d(transform, bottom_right_screen) else {
        return;
    };

    // Construct the bounds
    let left = top_left.x.min(bottom_right.x);
    let right = top_left.x.max(bottom_right.x);
    let bottom = top_left.y.min(bottom_right.y);
    let top = top_left.y.max(bottom_right.y);

    if !settings.show {
        return;
    }

    // Dynamic Spacing Calculation based on Zoom
    // We want major grid lines approx every 100 screen pixels.
    let scale = if let Projection::Orthographic(ortho) = projection {
        ortho.scale
    } else {
        1.0
    };
    let target_spacing = 100.0 * scale;
    let exponent = target_spacing.log10().floor();
    let major_spacing = 10.0_f32.powf(exponent);
    let minor_spacing = major_spacing / 10.0;

    // Update grid settings with current spacing for tools to use
    // Use minor spacing for finer snapping
    settings.spacing = minor_spacing;

    match settings.grid_type {
        GridType::Rectangular => draw_rectangular_grid(major_spacing, minor_spacing, left, right, bottom, top, &mut gizmos),
        GridType::Isometric => {
            // Placeholder for future implementation
        }
    }
}

fn draw_rectangular_grid(
    major_spacing: f32,
    minor_spacing: f32,
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    gizmos: &mut Gizmos,
) {
    let draw_lines = |spacing: f32, alpha: f32, gizmos: &mut Gizmos| {
        let start_x = (left / spacing).floor() * spacing;
        let start_y = (bottom / spacing).floor() * spacing;
        let count_x = ((right - left) / spacing).ceil() as i32 + 1;
        let count_y = ((top - bottom) / spacing).ceil() as i32 + 1;
        let color = Color::srgba(1.0, 1.0, 1.0, alpha);

        for i in 0..=count_x {
            let x = start_x + (i as f32) * spacing;
            gizmos.line_2d(Vec2::new(x, bottom), Vec2::new(x, top), color);
        }

        for i in 0..=count_y {
            let y = start_y + (i as f32) * spacing;
            gizmos.line_2d(Vec2::new(left, y), Vec2::new(right, y), color);
        }
    };

    // Draw minor lines (faint)
    if minor_spacing > 0.001 {
        draw_lines(minor_spacing, 0.05, gizmos);
    }

    // Draw major lines (stronger)
    draw_lines(major_spacing, 0.15, gizmos);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Vec2::new(1.1, 1.1), 1.0, Vec2::new(1.0, 1.0))]
    #[case(Vec2::new(1.6, 1.6), 1.0, Vec2::new(2.0, 2.0))]
    #[case(Vec2::new(0.0, 0.0), 1.0, Vec2::new(0.0, 0.0))]
    #[case(Vec2::new(1.0, 1.0), 0.0, Vec2::new(1.0, 1.0))] // Spacing 0 should not snap
    fn test_snap_to_grid(#[case] pos: Vec2, #[case] spacing: f32, #[case] expected: Vec2) {
        let result = snap_to_grid(pos, spacing);
        assert!((result.x - expected.x).abs() < 0.0001);
        assert!((result.y - expected.y).abs() < 0.0001);
    }
}
