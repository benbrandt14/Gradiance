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
    pub snap_to_grid: bool,
    /// Whether to automatically adjust spacing based on zoom.
    pub auto_spacing: bool,
    /// The distance between grid lines (in world units).
    pub spacing: f32,
    /// The type of grid to display.
    pub grid_type: GridType,

    // --- Object Snapping ---
    /// Master switch for object snapping.
    pub snap_to_objects: bool,
    /// Snap to object centers (transforms).
    pub snap_to_object_centers: bool,
    /// Snap to object corners (vertices).
    pub snap_to_object_corners: bool,
    /// Snap to object edges (nearest point on collider).
    pub snap_to_object_edges: bool,
    /// Snap to object midpoints (e.g. middle of an edge).
    pub snap_to_object_midpoints: bool,
    /// Maximum distance for snapping (in world units).
    pub snap_distance: f32,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            show: true,
            snap_to_grid: true,
            auto_spacing: true,
            spacing: 1.0,
            grid_type: GridType::Rectangular,

            snap_to_objects: false,
            snap_to_object_centers: true,
            snap_to_object_corners: false,
            snap_to_object_edges: false,
            snap_to_object_midpoints: false,
            snap_distance: 0.5,
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
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<Camera>>,
    window_query: Query<&Window, With<PrimaryWindow>>,
    mut gizmos: Gizmos,
) {
    let Some((camera, transform, projection)) = camera_query.iter().next() else {
        return;
    };

    let Some(window) = window_query.iter().next() else {
        return;
    };

    // Calculate visible area on Z=0 plane
    let get_world_pos = |screen_pos: Vec2| -> Option<Vec2> {
        if let Ok(pos) = camera.viewport_to_world_2d(transform, screen_pos) {
            return Some(pos);
        }
        if let Ok(ray) = camera.viewport_to_world(transform, screen_pos) {
            let normal = Vec3::Z;
            let denom = ray.direction.dot(normal);
            if denom.abs() > f32::EPSILON {
                 let t = (Vec3::ZERO - ray.origin).dot(normal) / denom;
                 if t >= 0.0 {
                     return Some(ray.get_point(t).truncate());
                 }
            }
        }
        None
    };

    let top_left_screen = Vec2::ZERO;
    let bottom_right_screen = Vec2::new(window.width(), window.height());

    let Some(p1) = get_world_pos(top_left_screen) else { return; };
    let Some(p2) = get_world_pos(bottom_right_screen) else { return; };
    // Also check other corners for perspective distortion
    let Some(p3) = get_world_pos(Vec2::new(window.width(), 0.0)) else { return; };
    let Some(p4) = get_world_pos(Vec2::new(0.0, window.height())) else { return; };

    // Construct the bounds covering all corners
    let left = p1.x.min(p2.x).min(p3.x).min(p4.x);
    let right = p1.x.max(p2.x).max(p3.x).max(p4.x);
    let bottom = p1.y.min(p2.y).min(p3.y).min(p4.y);
    let top = p1.y.max(p2.y).max(p3.y).max(p4.y);

    if !settings.show {
        return;
    }

    // Dynamic Spacing Calculation based on Zoom
    let scale = if let Projection::Orthographic(ortho) = projection {
        ortho.scale
    } else {
        // Perspective approximation: scale based on height
        transform.translation().z.abs() / 20.0
    };
    let target_spacing: f32 = 100.0 * scale;
    let exponent = target_spacing.log10().floor();
    let major_spacing = 10.0_f32.powf(exponent);
    let minor_spacing = major_spacing / 10.0;

    // Update grid settings with current spacing for tools to use only if auto_spacing is enabled
    if settings.auto_spacing {
        settings.spacing = minor_spacing;
    }

    let render_spacing = if settings.auto_spacing {
        minor_spacing
    } else {
        settings.spacing
    };
    let render_major_spacing = if settings.auto_spacing {
        major_spacing
    } else {
        settings.spacing * 10.0
    };

    match settings.grid_type {
        GridType::Rectangular => draw_rectangular_grid(
            render_major_spacing,
            render_spacing,
            left,
            right,
            bottom,
            top,
            &mut gizmos,
        ),
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
    // Draw at Z = -1.0 to ensure it's slightly behind 2D objects (at Z=0.0)
    // -10.0 might be too far if the camera is close or at a shallow angle
    let z_depth = -1.0;

    let draw_lines = |spacing: f32, alpha: f32, gizmos: &mut Gizmos| {
        let start_x = (left / spacing).floor() * spacing;
        let start_y = (bottom / spacing).floor() * spacing;
        let count_x = ((right - left) / spacing).ceil() as i32 + 1;
        let count_y = ((top - bottom) / spacing).ceil() as i32 + 1;
        let color = Color::srgba(1.0, 1.0, 1.0, alpha);

        for i in 0..=count_x {
            let x = start_x + (i as f32) * spacing;
            gizmos.line(
                Vec3::new(x, bottom, z_depth),
                Vec3::new(x, top, z_depth),
                color,
            );
        }

        for i in 0..=count_y {
            let y = start_y + (i as f32) * spacing;
            gizmos.line(
                Vec3::new(left, y, z_depth),
                Vec3::new(right, y, z_depth),
                color,
            );
        }
    };

    // Draw minor lines (faint)
    if minor_spacing > 0.001 {
        draw_lines(minor_spacing, 0.1, gizmos);
    }

    // Draw major lines (stronger)
    draw_lines(major_spacing, 0.4, gizmos);
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
