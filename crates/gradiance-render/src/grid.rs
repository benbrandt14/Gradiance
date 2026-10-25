//! Settings-driven reference grid: Cartesian, isometric, or polar, with
//! zoom-adaptive spacing (line density stays comfortable on screen). The
//! spacing math lives in `geometry::snapping` so snapping shares it.

use bevy::prelude::*;
use gradiance_core::constants::INTERACTION_PLANE_Z;
use gradiance_domain::settings::{GridSettings, GridSystem};
use gradiance_geometry::snapping::{
    CARTESIAN_LINE_DIRECTIONS, ISOMETRIC_LINE_DIRECTIONS, adaptive_step, is_major_line,
};
use gradiance_interaction::camera::{CameraRig, CameraScale};
use gradiance_interaction::overlay::GridGizmos;

/// Draws the configured grid at the zoom-adapted step
/// ([`adaptive_step`]) — the same step, from the same [`CameraScale`],
/// that `interaction::snap` snaps to, so the cursor lands on exactly the
/// lines drawn. Works under both projections (`CameraScale` is defined for
/// each); the grid is centred on the rig's focus, which stays on the view
/// when the camera orbits or dollies.
pub fn draw_grid(
    grid: Res<GridSettings>,
    cam_scale: Res<CameraScale>,
    rig: Res<CameraRig>,
    mut gizmos: Gizmos<GridGizmos>,
) {
    if !grid.visible {
        return;
    }
    let scale = cam_scale.0;
    let center = rig.focus.truncate();
    // Visible half-extent (generous; gizmo lines are cheap).
    let half = 800.0 * scale;

    // No valid spacing, no grid (and no unbounded line/ring loops below).
    let Some(spacing) = adaptive_step(grid.spacing, scale) else {
        return;
    };
    // Mid-value tint + low alpha: visible over the light backdrop without
    // clashing on dark bodies (the grid is an overlay, not a surface).
    let minor = Color::srgba(0.55, 0.57, 0.62, 0.14);
    // Majors (`geometry::snapping::is_major_line`) sit at fixed world
    // positions as the step doubles; same tint, stronger alpha.
    let major = minor.with_alpha(0.35);
    let axis = Color::srgba(0.55, 0.57, 0.62, 0.4);

    let families = match grid.system {
        GridSystem::Cartesian => Some(CARTESIAN_LINE_DIRECTIONS),
        GridSystem::Isometric => Some(ISOMETRIC_LINE_DIRECTIONS),
        GridSystem::Polar { .. } => None,
    };
    if let Some(dirs) = families {
        let rot = Vec2::from_angle(grid.rotation);
        for dir in dirs {
            line_family(
                &mut gizmos,
                grid.origin,
                rot.rotate(dir),
                spacing,
                center,
                half,
                [minor, major, axis],
            );
        }
    }
    if let GridSystem::Polar { angular_divisions } = grid.system {
        let max_radius = (center - grid.origin).length() + half * 1.5;
        // Ring `i` at `i · spacing` (multiplied, not accumulated, so a major
        // ring's radius is exact); majors by the same rule as lines.
        let mut i = 1_i32;
        while i as f32 * spacing <= max_radius {
            gizmos.circle(
                Isometry3d::from_translation(grid.origin.extend(INTERACTION_PLANE_Z)),
                i as f32 * spacing,
                if is_major_line(i) { major } else { minor },
            );
            i += 1;
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

/// Draws one family of parallel lines (unit direction `dir`) spaced
/// `spacing` apart, covering a disc of `half` around `center`. `colors` is
/// `[minor, major, axis]`: the line through the grid origin draws in the
/// axis color, other major lines ([`is_major_line`]) in the major color.
fn line_family(
    gizmos: &mut Gizmos<GridGizmos>,
    origin: Vec2,
    dir: Vec2,
    spacing: f32,
    center: Vec2,
    half: f32,
    [minor, major, axis]: [Color; 3],
) {
    let normal = dir.perp();
    // Signed distance of the view center from the origin along the normal.
    let center_offset = (center - origin).dot(normal);
    let first = ((center_offset - half * 1.5) / spacing).floor() as i32;
    let last = ((center_offset + half * 1.5) / spacing).ceil() as i32;
    let reach = half * 1.6;
    for i in first..=last {
        let base = origin + normal * (i as f32 * spacing);
        let color = if i == 0 {
            axis
        } else if is_major_line(i) {
            major
        } else {
            minor
        };
        gizmos.line(
            (base - dir * reach).extend(INTERACTION_PLANE_Z),
            (base + dir * reach).extend(INTERACTION_PLANE_Z),
            color,
        );
    }
}
