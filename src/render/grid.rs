//! Settings-driven reference grid: Cartesian, isometric, or polar, with
//! zoom-adaptive spacing (line density stays comfortable on screen).

use crate::core::constants::INTERACTION_PLANE_Z;
use crate::domain::settings::{GridSettings, GridSystem};
use crate::render::overlay::GridGizmos;
use bevy::prelude::*;

/// Preferred on-screen line spacing band, in logical pixels.
const MIN_SCREEN_SPACING: f32 = 24.0;
const MAX_SCREEN_SPACING: f32 = 96.0;

/// Draws the configured grid, adapting spacing to the camera zoom by
/// powers of two of the configured base spacing.
pub fn draw_grid(
    grid: Res<GridSettings>,
    cameras: Query<(&Transform, &Projection), With<Camera3d>>,
    mut gizmos: Gizmos<GridGizmos>,
) {
    if !grid.visible {
        return;
    }
    let Some((cam_transform, Projection::Orthographic(ortho))) = cameras.iter().next() else {
        return;
    };
    let scale = ortho.scale;
    let center = cam_transform.translation.truncate();
    // Visible half-extent (generous; gizmo lines are cheap).
    let half = 800.0 * scale;

    let spacing = adaptive_spacing(grid.spacing, scale);
    // Mid-value tint + low alpha: visible over the light backdrop without
    // clashing on dark bodies (the grid is an overlay, not a surface).
    let minor = Color::srgba(0.55, 0.57, 0.62, 0.14);
    let axis = Color::srgba(0.55, 0.57, 0.62, 0.4);

    match grid.system {
        GridSystem::Cartesian => {
            let dirs = [grid.rotation, grid.rotation + std::f32::consts::FRAC_PI_2];
            for (i, angle) in dirs.into_iter().enumerate() {
                line_family(
                    &mut gizmos,
                    grid.origin,
                    angle,
                    spacing,
                    center,
                    half,
                    minor,
                    axis,
                );
                let _ = i;
            }
        }
        GridSystem::Isometric => {
            for offset in [30_f32.to_radians(), 150_f32.to_radians()] {
                line_family(
                    &mut gizmos,
                    grid.origin,
                    grid.rotation + offset,
                    spacing,
                    center,
                    half,
                    minor,
                    axis,
                );
            }
        }
        GridSystem::Polar { angular_divisions } => {
            let max_radius = (center - grid.origin).length() + half * 1.5;
            let mut r = spacing;
            while r <= max_radius {
                gizmos.circle(
                    Isometry3d::from_translation(grid.origin.extend(INTERACTION_PLANE_Z)),
                    r,
                    minor,
                );
                r += spacing;
            }
            let step = std::f32::consts::TAU / angular_divisions.max(1) as f32;
            for i in 0..angular_divisions.max(1) {
                let dir = Vec2::from_angle(grid.rotation + step * i as f32);
                gizmos.line(
                    grid.origin.extend(INTERACTION_PLANE_Z),
                    (grid.origin + dir * max_radius).extend(INTERACTION_PLANE_Z),
                    minor,
                );
            }
        }
    }
}

/// Scales base spacing by powers of two until on-screen spacing falls in
/// the preferred band.
fn adaptive_spacing(base: f32, camera_scale: f32) -> f32 {
    let mut spacing = base.max(f32::EPSILON);
    let mut guard = 0;
    while spacing / camera_scale < MIN_SCREEN_SPACING && guard < 32 {
        spacing *= 2.0;
        guard += 1;
    }
    while spacing / camera_scale > MAX_SCREEN_SPACING && guard < 64 {
        spacing /= 2.0;
        guard += 1;
    }
    spacing
}

/// Draws one family of parallel lines (direction `angle`) spaced
/// `spacing` apart, covering a disc of `half` around `center`. The line
/// through the grid origin draws in the `axis` color.
#[expect(clippy::too_many_arguments)]
fn line_family(
    gizmos: &mut Gizmos<GridGizmos>,
    origin: Vec2,
    angle: f32,
    spacing: f32,
    center: Vec2,
    half: f32,
    minor: impl Into<Color> + Copy,
    axis: impl Into<Color> + Copy,
) {
    let dir = Vec2::from_angle(angle);
    let normal = dir.perp();
    // Signed distance of the view center from the origin along the normal.
    let center_offset = (center - origin).dot(normal);
    let first = ((center_offset - half * 1.5) / spacing).floor() as i32;
    let last = ((center_offset + half * 1.5) / spacing).ceil() as i32;
    let reach = half * 1.6;
    for i in first..=last {
        let base = origin + normal * (i as f32 * spacing);
        // Major line every 5th lattice step; origin line in axis color.
        let color: Color = if i == 0 {
            axis.into()
        } else if i.rem_euclid(5) == 0 {
            let c: Color = minor.into();
            c.with_alpha(c.alpha() * 2.5)
        } else {
            minor.into()
        };
        gizmos.line(
            (base - dir * reach).extend(INTERACTION_PLANE_Z),
            (base + dir * reach).extend(INTERACTION_PLANE_Z),
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_spacing_stays_in_screen_band() {
        for zoom in [0.05_f32, 0.3, 1.0, 4.0, 19.0] {
            let s = adaptive_spacing(100.0, zoom) / zoom;
            assert!(
                (MIN_SCREEN_SPACING..=MAX_SCREEN_SPACING).contains(&s),
                "screen spacing {s} at zoom {zoom}"
            );
        }
    }
}
