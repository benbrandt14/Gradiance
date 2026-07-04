//! Editor feedback gizmos: snap glyphs, constraint guides, selection
//! outlines.
//!
//! CAD-style: each snap kind draws a distinct glyph so the user always
//! knows *what* they are about to snap to.

use crate::domain::Body;
use crate::domain::shape::ShapeDef;
use crate::geometry::polygonize::polygonize;
use crate::interaction::gesture::{AxisConstraint, GestureConstraints};
use crate::interaction::selection::Selection;
use crate::interaction::snap::{SnapKind, SnappedCursor};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Screen-constant glyph half-size in world units.
fn glyph_size(projections: &Query<&Projection, With<Camera3d>>) -> f32 {
    6.0 * crate::interaction::camera::camera_scale(projections)
}

/// Draws the active snap point with a kind-specific glyph.
pub fn draw_snap_indicator(
    snapped: Res<SnappedCursor>,
    projections: Query<&Projection, With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    let (Some(pos), Some(kind)) = (snapped.position, snapped.kind) else {
        return;
    };
    let s = glyph_size(&projections);
    let color = css::GOLD;
    match kind {
        SnapKind::Grid => {
            gizmos.line_2d(pos - Vec2::X * s, pos + Vec2::X * s, color);
            gizmos.line_2d(pos - Vec2::Y * s, pos + Vec2::Y * s, color);
        }
        SnapKind::Vertex => {
            gizmos.rect_2d(
                Isometry2d::from_translation(pos),
                Vec2::splat(s * 2.0),
                color,
            );
        }
        SnapKind::Midpoint => {
            let a = pos + Vec2::new(0.0, s);
            let b = pos + Vec2::new(-s, -s);
            let c = pos + Vec2::new(s, -s);
            gizmos.line_2d(a, b, color);
            gizmos.line_2d(b, c, color);
            gizmos.line_2d(c, a, color);
        }
        SnapKind::Center => {
            gizmos.circle_2d(Isometry2d::from_translation(pos), s, color);
            gizmos.line_2d(pos - Vec2::X * s, pos + Vec2::X * s, color);
        }
        SnapKind::Edge => {
            gizmos.circle_2d(Isometry2d::from_translation(pos), s * 0.6, color);
        }
    }
}

/// Draws axis-constraint guide lines through the cursor while a
/// constraint is active.
pub fn draw_constraint_guides(
    constraints: Res<GestureConstraints>,
    snapped: Res<SnappedCursor>,
    mut gizmos: Gizmos,
) {
    const LEN: f32 = 100_000.0;
    let Some(pos) = snapped.effective() else {
        return;
    };
    let color = css::CORNFLOWER_BLUE.with_alpha(0.4);
    match constraints.axis {
        AxisConstraint::Horizontal => {
            gizmos.line_2d(pos - Vec2::X * LEN, pos + Vec2::X * LEN, color);
        }
        AxisConstraint::Vertical => {
            gizmos.line_2d(pos - Vec2::Y * LEN, pos + Vec2::Y * LEN, color);
        }
        AxisConstraint::Dominant => {
            gizmos.line_2d(pos - Vec2::X * LEN, pos + Vec2::X * LEN, color);
            gizmos.line_2d(pos - Vec2::Y * LEN, pos + Vec2::Y * LEN, color);
        }
        AxisConstraint::Free => {}
    }
}

/// Outlines selected bodies with their world-space contours.
pub fn draw_selection_outlines(
    selection: Res<Selection>,
    bodies: Query<(&ShapeDef, &Transform), With<Body>>,
    mut gizmos: Gizmos,
) {
    for entity in selection.iter() {
        let Ok((shape, transform)) = bodies.get(entity) else {
            continue;
        };
        let affine = transform.compute_affine();
        let contours = polygonize(shape);
        for ring in contours.rings() {
            let mut points: Vec<Vec2> = ring
                .iter()
                .map(|v| affine.transform_point3(v.extend(0.0)).truncate())
                .collect();
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            gizmos.linestrip_2d(points, css::ORANGE);
        }
    }
}
