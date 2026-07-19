//! Joint indicator gizmos: a glyph per joint kind, tracking the bodies.
//!
//! The selected joint is ringed in yellow, and a powered motor draws a
//! direction arrow (curved for hinges, straight for sliders) whose
//! length scales with target velocity — the "visualize motor state" the
//! joint feedback asked for.

use crate::core::ids::IdIndex;
use crate::core::units::PosRot;
use crate::domain::joint::{JointDef, JointKind, MotorDef};
use crate::interaction::joint_edit::{
    HINGE_LIMIT_RADIUS_PX, HINGE_RING_PX, JointLimitDrag, slider_span,
};
use crate::interaction::selection::SelectedJoint;
use crate::render::overlay::OverlayGizmos;
use bevy::color::palettes::css;
use bevy::prelude::*;

/// Draws every joint's anchor glyph (hinge = ring, weld = square,
/// slider = axis line), following the connected bodies.
pub fn draw_joints(
    joints: Query<(Entity, &JointDef)>,
    selected: Res<SelectedJoint>,
    limit_drag: Res<JointLimitDrag>,
    index: Res<IdIndex>,
    transforms: Query<&Transform>,
    cam_scale: Res<crate::interaction::camera::CameraScale>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    // Screen-constant glyph size: readable at any zoom.
    let s = cam_scale.0;
    for (entity, def) in &joints {
        let Some(entity_a) = index.entity(def.body_a) else {
            continue;
        };
        let Ok(pose_a) = transforms.get(entity_a).map(PosRot::from_transform) else {
            continue;
        };
        let anchor = def.anchor_world(pose_a.pos, pose_a.rot);
        // Grey glyphs with a dark under-stroke for contrast on any body
        // color (feedback 4.7); world pins keep their warning hue.
        let color = if def.body_b.is_none() {
            css::ORANGE_RED // world pin
        } else {
            css::LIGHT_GRAY
        };
        match &def.kind {
            JointKind::Hinge { motor, limits } => {
                // Under-stroke: a slightly offset dark ring reads as an
                // outline at every zoom (gizmo lines have no width knob).
                let r = HINGE_RING_PX * s;
                gizmos.circle_2d(Isometry2d::from_translation(anchor), r * 1.1, OUTLINE);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), r, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 1.5 * s, color);
                // Exact allowed-rotation range (relative to body A), with
                // grab handles at the ends when the joint is selected. A
                // live handle drag previews its tentative range instead.
                let (limits, live) = tentative_or(&limit_drag, entity, *limits);
                if let Some([min, max]) = limits {
                    let arc_color = if live {
                        css::AQUAMARINE
                    } else {
                        css::CORNFLOWER_BLUE
                    };
                    draw_limit_arc(
                        &mut gizmos,
                        anchor,
                        def.limit_reference_angle(pose_a.rot),
                        [min, max],
                        s,
                        arc_color,
                        selected.0 == Some(entity),
                    );
                }
                if let Some(m) = motor {
                    draw_angular_motor(&mut gizmos, anchor, *m, s);
                }
            }
            JointKind::Slider {
                axis,
                motor,
                limits,
            } => {
                let dir = Vec2::from_angle(pose_a.rot).rotate(*axis);
                // With travel limits the axis line shows the exact allowed
                // travel (plus end caps); unlimited prismatics keep the
                // long reference line. A live handle drag previews its
                // tentative travel instead.
                let (limits, live) = tentative_or(&limit_drag, entity, *limits);
                let span_color = if live { css::AQUAMARINE } else { color };
                let (from, to) = slider_span(anchor, dir, limits, s);
                let perp = Vec2::new(-dir.y, dir.x) * 4.0 * s;
                gizmos.line_2d(from, to, span_color);
                if limits.is_some() {
                    gizmos.line_2d(from - perp, from + perp, span_color);
                    gizmos.line_2d(to - perp, to + perp, span_color);
                    if selected.0 == Some(entity) {
                        // Grab handles on the travel caps.
                        gizmos.circle_2d(Isometry2d::from_translation(from), 3.0 * s, css::GOLD);
                        gizmos.circle_2d(Isometry2d::from_translation(to), 3.0 * s, css::GOLD);
                    }
                }
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 4.4 * s, OUTLINE);
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

/// Dark under-stroke color for glyph outlines.
const OUTLINE: bevy::color::Srgba = css::DARK_SLATE_GRAY;

/// The limits to draw for `entity`: the live drag's tentative range when
/// this joint's handle is being dragged (flagged `true`), else the
/// authored limits.
fn tentative_or(
    drag: &JointLimitDrag,
    entity: Entity,
    authored: Option<[f32; 2]>,
) -> (Option<[f32; 2]>, bool) {
    match (drag.dragging() == Some(entity), drag.tentative) {
        (true, Some(t)) => (Some(t), true),
        _ => (authored, false),
    }
}

/// The hinge's allowed-rotation arc (world angles `rot_a + min..max`), with
/// end ticks, and grab-handle dots when `with_handles`.
fn draw_limit_arc(
    gizmos: &mut Gizmos<OverlayGizmos>,
    anchor: Vec2,
    rot_a: f32,
    [min, max]: [f32; 2],
    s: f32,
    color: bevy::color::Srgba,
    with_handles: bool,
) {
    let r = HINGE_LIMIT_RADIUS_PX * s;
    let span = (max - min).max(1e-3);
    let steps = (span.abs() * 8.0).ceil().max(2.0) as usize;
    let at = |a: f32| anchor + Vec2::from_angle(rot_a + a) * r;
    let mut prev = at(min);
    for i in 1..=steps {
        let next = at(min + span * (i as f32 / steps as f32));
        gizmos.line_2d(prev, next, color);
        prev = next;
    }
    // End ticks crossing the arc, plus handle dots when selected.
    for a in [min, max] {
        let dir = Vec2::from_angle(rot_a + a);
        gizmos.line_2d(
            anchor + dir * (r - 3.0 * s),
            anchor + dir * (r + 3.0 * s),
            color,
        );
        if with_handles {
            gizmos.circle_2d(Isometry2d::from_translation(at(a)), 3.0 * s, css::GOLD);
        }
    }
}

/// A curved arrow around the hinge showing spin direction & strength.
fn draw_angular_motor(gizmos: &mut Gizmos<OverlayGizmos>, anchor: Vec2, motor: MotorDef, s: f32) {
    if !motor.enabled || motor.target_velocity.abs() < 1e-3 {
        return;
    }
    let radius = 14.0 * s;
    // Sweep grows with drive speed: direction *and* magnitude at a glance
    // (feedback 4.1 "visually indicate motor state").
    let sweep =
        motor.target_velocity.signum() * (1.2 + motor.target_velocity.abs().min(10.0) * 0.16); // 1.2..2.8 rad
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
fn draw_spring(gizmos: &mut Gizmos<OverlayGizmos>, a: Vec2, b: Vec2, s: f32) {
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
    gizmos: &mut Gizmos<OverlayGizmos>,
    anchor: Vec2,
    dir: Vec2,
    motor: MotorDef,
    s: f32,
) {
    if !motor.enabled || motor.target_velocity.abs() < 1e-3 {
        return;
    }
    let d = dir * motor.target_velocity.signum();
    // Arrow length grows with drive speed (16..34 px at screen scale).
    let tip = anchor + d * (16.0 + motor.target_velocity.abs().min(600.0) * 0.03) * s;
    gizmos.line_2d(anchor, tip, css::GOLD);
    let back = -d;
    let perp = Vec2::new(-d.y, d.x);
    gizmos.line_2d(tip, tip + (back + perp) * 6.0 * s, css::GOLD);
    gizmos.line_2d(tip, tip + (back - perp) * 6.0 * s, css::GOLD);
}
