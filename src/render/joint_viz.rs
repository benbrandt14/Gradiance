//! Joint indicator gizmos: a glyph per joint kind, tracking the bodies.
//!
//! The selected joint is ringed in yellow, and a powered motor draws a
//! direction arrow (curved for hinges, straight for sliders) whose
//! length scales with target velocity — the "visualize motor state" the
//! joint feedback asked for.

use crate::core::ids::IdIndex;
use crate::core::units::PosRot;
use crate::domain::joint::{JointDef, JointKind, MotorDef};
use crate::interaction::selection::SelectedJoint;
use bevy::color::palettes::css;
use bevy::gizmos::config::GizmoConfigGroup;
use bevy::prelude::*;

/// Gizmo config group for joint indicators. Configured with a negative
/// `depth_bias` (see [`GradianceRenderPlugin`](crate::render)) so joint glyphs
/// always draw in front of the extruded body prisms rather than being occluded.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct JointGizmos;

/// Draws every joint's anchor glyph (hinge = ring, weld = square,
/// slider = axis line), following the connected bodies.
pub fn draw_joints(
    joints: Query<(Entity, &JointDef)>,
    selected: Res<SelectedJoint>,
    index: Res<IdIndex>,
    transforms: Query<&Transform>,
    projections: Query<&Projection, With<Camera3d>>,
    mut gizmos: Gizmos<JointGizmos>,
) {
    // Screen-constant glyph size: readable at any zoom.
    let s = crate::interaction::camera::camera_scale(&projections);
    for (entity, def) in &joints {
        let Some(entity_a) = index.entity(def.body_a) else {
            continue;
        };
        let Ok(pose_a) = transforms.get(entity_a).map(PosRot::from_transform) else {
            continue;
        };
        let anchor = def.anchor_world(pose_a.pos, pose_a.rot);
        let color = if def.body_b.is_none() {
            css::ORANGE_RED // world pin
        } else {
            css::VIOLET
        };
        match &def.kind {
            JointKind::Hinge { motor, .. } => {
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 6.0 * s, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 1.5 * s, color);
                if let Some(m) = motor {
                    draw_angular_motor(&mut gizmos, anchor, *m, s);
                }
            }
            JointKind::Slider { axis, motor, .. } => {
                let dir = Vec2::from_angle(pose_a.rot).rotate(*axis);
                gizmos.line_2d(anchor - dir * 40.0 * s, anchor + dir * 40.0 * s, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 4.0 * s, color);
                if let Some(m) = motor {
                    draw_linear_motor(&mut gizmos, anchor, dir, *m, s);
                }
            }
            JointKind::Spring { .. } => {
                // A coil between the two anchors; the connected bodies don't
                // collide, so it reads as a free spring, not a rod.
                let world_b = match def.body_b {
                    Some(id) => index
                        .entity(id)
                        .and_then(|e| transforms.get(e).ok())
                        .map(PosRot::from_transform)
                        .map(|pose_b| {
                            pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b)
                        }),
                    None => Some(def.anchor_b), // world pin
                };
                if let Some(b) = world_b {
                    draw_spring(&mut gizmos, anchor, b, s);
                }
            }
        }

        // Selection highlight.
        if selected.0 == Some(entity) {
            gizmos.circle_2d(Isometry2d::from_translation(anchor), 11.0 * s, css::YELLOW);
        }
    }
}

/// A curved arrow around the hinge showing spin direction & strength.
fn draw_angular_motor(gizmos: &mut Gizmos<JointGizmos>, anchor: Vec2, motor: MotorDef, s: f32) {
    if !motor.enabled || motor.target_velocity.abs() < 1e-3 {
        return;
    }
    let radius = 14.0 * s;
    let sweep = (motor.target_velocity.signum()) * 2.2; // radians of arc
    let steps = 10;
    let mut prev = anchor + Vec2::from_angle(0.0) * radius;
    for i in 1..=steps {
        let a = sweep * (i as f32 / steps as f32);
        let next = anchor + Vec2::from_angle(a) * radius;
        gizmos.line_2d(prev, next, css::GOLD);
        prev = next;
    }
    // Arrowhead at the sweep end.
    let tip = anchor + Vec2::from_angle(sweep) * radius;
    let tangent = Vec2::from_angle(sweep + std::f32::consts::FRAC_PI_2) * sweep.signum();
    gizmos.line_2d(
        tip,
        tip - tangent * 5.0 * s + (anchor - tip).normalize() * 4.0 * s,
        css::GOLD,
    );
    gizmos.line_2d(
        tip,
        tip - tangent * 5.0 * s - (anchor - tip).normalize() * 4.0 * s,
        css::GOLD,
    );
}

/// Draws a spring coil between two world points. Straight lead-ins at each end,
/// a zigzag body in the middle, and a dot at each anchor. Amplitude is
/// screen-constant (scaled by `s`); the coil count is fixed.
fn draw_spring(gizmos: &mut Gizmos<JointGizmos>, a: Vec2, b: Vec2, s: f32) {
    const COILS: usize = 8;
    let color = css::SPRING_GREEN;
    let span = b - a;
    let len = span.length();
    gizmos.circle_2d(Isometry2d::from_translation(a), 3.0 * s, color);
    gizmos.circle_2d(Isometry2d::from_translation(b), 3.0 * s, color);
    if len < 1e-3 {
        return;
    }
    let unit = span / len;
    let perp = Vec2::new(-unit.y, unit.x);
    let amp = 5.0 * s;
    // Straight lead-ins so the coil doesn't start right at the anchors.
    let lead = (len * 0.2).min(12.0 * s);
    let start = a + unit * lead;
    let end = b - unit * lead;
    gizmos.line_2d(a, start, color);
    gizmos.line_2d(end, b, color);
    let seg = (end - start) / COILS as f32;
    let mut prev = start;
    for i in 1..COILS {
        let along = start + seg * i as f32;
        let side = if i % 2 == 1 { amp } else { -amp };
        let point = along + perp * side;
        gizmos.line_2d(prev, point, color);
        prev = point;
    }
    gizmos.line_2d(prev, end, color);
}

/// A straight arrow along the slider axis showing drive direction.
fn draw_linear_motor(
    gizmos: &mut Gizmos<JointGizmos>,
    anchor: Vec2,
    dir: Vec2,
    motor: MotorDef,
    s: f32,
) {
    if !motor.enabled || motor.target_velocity.abs() < 1e-3 {
        return;
    }
    let d = dir * motor.target_velocity.signum();
    let tip = anchor + d * 26.0 * s;
    gizmos.line_2d(anchor, tip, css::GOLD);
    let back = -d;
    let perp = Vec2::new(-d.y, d.x);
    gizmos.line_2d(tip, tip + (back + perp) * 6.0 * s, css::GOLD);
    gizmos.line_2d(tip, tip + (back - perp) * 6.0 * s, css::GOLD);
}
