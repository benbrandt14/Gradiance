//! Joint indicator gizmos: a glyph per joint kind, tracking the bodies.

use crate::core::ids::IdIndex;
use crate::core::units::PosRot;
use crate::domain::joint::{JointDef, JointKind};
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Draws every joint's anchor glyph (hinge = ring, weld = square,
/// slider = axis line), following the connected bodies.
pub fn draw_joints(
    joints: Query<&JointDef>,
    index: Res<IdIndex>,
    transforms: Query<&Transform>,
    projections: Query<&Projection, With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    // Screen-constant glyph size: readable at any zoom.
    let s = crate::interaction::camera::camera_scale(&projections);
    for def in &joints {
        let Some(entity_a) = index.entity(def.body_a) else {
            continue;
        };
        let Ok(pose_a) = transforms.get(entity_a).map(PosRot::from_transform) else {
            continue;
        };
        let anchor = pose_a.pos + Vec2::from_angle(pose_a.rot).rotate(def.anchor_a);
        let color = if def.body_b.is_none() {
            css::ORANGE_RED // world pin
        } else {
            css::VIOLET
        };
        match &def.kind {
            JointKind::Hinge { .. } => {
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 6.0 * s, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 1.5 * s, color);
            }
            JointKind::Weld => {
                gizmos.rect_2d(
                    Isometry2d::new(anchor, Rot2::radians(pose_a.rot)),
                    Vec2::splat(9.0 * s),
                    color,
                );
            }
            JointKind::Slider { axis, .. } => {
                let dir = Vec2::from_angle(pose_a.rot).rotate(*axis);
                gizmos.line_2d(anchor - dir * 40.0 * s, anchor + dir * 40.0 * s, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 4.0 * s, color);
            }
        }
    }
}
