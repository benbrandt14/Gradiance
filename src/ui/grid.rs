//! Grid and snapping system.

use crate::input::{cursor::CursorWorldPos, ZIndex};
use crate::prelude::*;
use bevy::math::DVec2;
use bevy_egui::{EguiContexts, egui};

/// Plugin that handles the grid visualization and snapping.
pub struct GridPlugin;

impl Plugin for GridPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GridSettings>();
        app.add_systems(Update, (draw_grid, grid_ui));
    }
}

/// Settings for the grid system.
#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct GridSettings {
    /// Whether the grid is visible.
    pub show: bool,
    /// Whether snapping is enabled.
    pub snap: bool,
    /// The spacing of the grid lines (e.g., 10.0 means a line every 10 units).
    /// This is the "base" spacing. Actual drawn lines adapt to zoom.
    pub spacing: f64,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            show: true,
            snap: true,
            spacing: 1.0, // 1 meter default (100 px)
        }
    }
}

/// Snaps a position to the nearest grid point.
pub fn snap_to_grid(pos: DVec2, spacing: f64) -> DVec2 {
    let x = (pos.x / spacing).round() * spacing;
    let y = (pos.y / spacing).round() * spacing;
    DVec2::new(x, y)
}

fn grid_ui(mut contexts: EguiContexts, mut settings: ResMut<GridSettings>) {
    egui::Window::new("Grid")
        .default_open(false)
        .show(contexts.ctx_mut(), |ui| {
            ui.checkbox(&mut settings.show, "Show Grid");
            ui.checkbox(&mut settings.snap, "Snap to Grid");
            ui.add(egui::DragValue::new(&mut settings.spacing).speed(0.1).range(0.1..=100.0).prefix("Spacing: "));
        });
}

fn draw_grid(
    settings: Res<GridSettings>,
    mut gizmos: Gizmos,
    q_camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    window: Query<&Window>,
) {
    if !settings.show {
        return;
    }

    let (camera, transform) = match q_camera.iter().next() {
        Some(c) => c,
        None => return,
    };

    let window = match window.iter().next() {
        Some(w) => w,
        None => return,
    };

    // Calculate visible world area
    let viewport_size = Vec2::new(window.width(), window.height());
    let top_left_screen = Vec2::ZERO;
    let bottom_right_screen = viewport_size;

    let top_left = if let Some(p) = camera.viewport_to_world_2d(transform, top_left_screen) {
        p
    } else {
        return;
    };

    let bottom_right = if let Some(p) = camera.viewport_to_world_2d(transform, bottom_right_screen) {
        p
    } else {
        return;
    };

    let min_x = top_left.x.min(bottom_right.x) as f64;
    let max_x = top_left.x.max(bottom_right.x) as f64;
    let min_y = top_left.y.min(bottom_right.y) as f64;
    let max_y = top_left.y.max(bottom_right.y) as f64;

    // Adaptive Grid Spacing
    // We want lines to be roughly 20-100 pixels apart on screen.
    // 1 unit = 100 pixels (roughly) at scale 1.0?
    // Wait, Bevy 2D camera scale affects this.
    // Let's just use the settings.spacing and scale it by powers of 10.

    let width = max_x - min_x;
    // Aim for ~10-20 lines horizontally
    let target_step = width / 20.0;

    // Find nearest power of 10 (or 2/5 multiples)
    let power = 10.0f64.powf(target_step.log10().floor());
    let mut step = power;
    if target_step / step > 5.0 {
        step *= 5.0;
    } else if target_step / step > 2.0 {
        step *= 2.0;
    }

    // Enforce minimum step based on settings
    if step < settings.spacing {
        step = settings.spacing;
    }

    // Align start to step
    let start_x = (min_x / step).floor() * step;
    let start_y = (min_y / step).floor() * step;

    let color = Color::srgb(0.2, 0.2, 0.2); // Faint grey

    let mut x = start_x;
    while x <= max_x {
        gizmos.line_2d(
            Vec2::new(x as f32, min_y as f32),
            Vec2::new(x as f32, max_y as f32),
            color,
        );
        x += step;
    }

    let mut y = start_y;
    while y <= max_y {
        gizmos.line_2d(
            Vec2::new(min_x as f32, y as f32),
            Vec2::new(max_x as f32, y as f32),
            color,
        );
        y += step;
    }
}
