//! Background reference grid (gizmo lines, immediate mode).

use bevy::color::palettes::css;
use bevy::prelude::*;

/// Grid line spacing in world pixels.
const SPACING: f32 = 100.0;
/// Half-extent of the drawn grid region.
const EXTENT: f32 = 2_000.0;

/// Draws a fixed-spacing reference grid with highlighted axes.
///
/// Camera-adaptive spacing arrives with the camera controller milestone.
pub fn draw_grid(mut gizmos: Gizmos) {
    let minor = css::DIM_GRAY.with_alpha(0.15);
    let steps = (EXTENT / SPACING) as i32;
    for i in -steps..=steps {
        let offset = i as f32 * SPACING;
        if i != 0 {
            gizmos.line_2d(Vec2::new(offset, -EXTENT), Vec2::new(offset, EXTENT), minor);
            gizmos.line_2d(Vec2::new(-EXTENT, offset), Vec2::new(EXTENT, offset), minor);
        }
    }
    // Axes.
    gizmos.line_2d(
        Vec2::new(0.0, -EXTENT),
        Vec2::new(0.0, EXTENT),
        css::DIM_GRAY.with_alpha(0.5),
    );
    gizmos.line_2d(
        Vec2::new(-EXTENT, 0.0),
        Vec2::new(EXTENT, 0.0),
        css::DIM_GRAY.with_alpha(0.5),
    );
}
