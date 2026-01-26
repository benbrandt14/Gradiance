//! Grid visualization and snapping settings.

use crate::prelude::*;

/// Settings for the editor grid.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct GridSettings {
    /// Whether the grid is visible.
    pub show: bool,
    /// Whether snapping is enabled.
    pub snap_to_grid: bool,
    /// Grid spacing in world units.
    pub spacing: f32,
    /// Grid color.
    pub color: Color,
    /// Alpha transparency of grid lines.
    pub alpha: f32,

    // Snapping options (Added back for compatibility)
    /// Snap distance threshold.
    pub snap_distance: f32,
    /// Enable object snapping.
    pub snap_to_objects: bool,
    /// Snap to object centers.
    pub snap_to_object_centers: bool,
    /// Snap to object corners.
    pub snap_to_object_corners: bool,
    /// Snap to object edges.
    pub snap_to_object_edges: bool,
    /// Snap to object midpoints.
    pub snap_to_object_midpoints: bool,

    /// Auto spacing (UI helper, not logic)
    pub auto_spacing: bool,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            show: true,
            snap_to_grid: true,
            spacing: 1.0,
            color: Color::srgb(0.3, 0.3, 0.3), // Dark Grey
            alpha: 0.3,
            snap_distance: 0.5,
            snap_to_objects: true,
            snap_to_object_centers: true,
            snap_to_object_corners: true,
            snap_to_object_edges: false,
            snap_to_object_midpoints: false,
            auto_spacing: false,
        }
    }
}

/// Plugin for Grid systems.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<GridSettings>();
        app.init_resource::<GridSettings>();
        app.add_systems(Update, draw_grid);
    }
}

/// System to draw the infinite 3D grid using Gizmos.
fn draw_grid(
    settings: Res<GridSettings>,
    mut gizmos: Gizmos,
    camera: Query<&GlobalTransform, With<Camera3d>>
) {
    if !settings.show {
        return;
    }

    // For "Infinite" feel, we can draw a grid around the camera's XY projection.
    let Ok(cam_t) = camera.get_single() else { return; };
    let cam_pos = cam_t.translation();

    // Snap center to grid spacing
    let spacing = settings.spacing;
    let center_x = (cam_pos.x / spacing).round() * spacing;
    let center_y = (cam_pos.y / spacing).round() * spacing;

    // Extents: how far to draw? Based on height maybe?
    let height = cam_pos.z.abs();
    let extent = (height * 3.0).max(100.0); // Draw enough to fill view

    let color = settings.color.with_alpha(settings.alpha);

    let min_x = center_x - extent;
    let max_x = center_x + extent;
    let min_y = center_y - extent;
    let max_y = center_y + extent;

    // X lines
    let mut x = min_x;
    while x <= max_x {
        gizmos.line(
            Vec3::new(x, min_y, 0.0),
            Vec3::new(x, max_y, 0.0),
            color
        );
        x += spacing;
    }

    // Y lines
    let mut y = min_y;
    while y <= max_y {
        gizmos.line(
            Vec3::new(min_x, y, 0.0),
            Vec3::new(max_x, y, 0.0),
            color
        );
        y += spacing;
    }

    // Draw axes
    gizmos.line(Vec3::new(-1000.0, 0.0, 0.0), Vec3::new(1000.0, 0.0, 0.0), Color::srgb(1.0, 0.0, 0.0)); // Red
    gizmos.line(Vec3::new(0.0, -1000.0, 0.0), Vec3::new(0.0, 1000.0, 0.0), Color::srgb(0.0, 1.0, 0.0)); // Green
}

/// Helper for snapping (exposed if needed, but snapping.rs implements it too now)
pub fn snap_to_grid(pos: Vec2, spacing: f32) -> Vec2 {
    let x = (pos.x / spacing).round() * spacing;
    let y = (pos.y / spacing).round() * spacing;
    Vec2::new(x, y)
}
