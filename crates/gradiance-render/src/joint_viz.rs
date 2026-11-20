//! Joint indicator gizmos: a glyph per joint kind, tracking the bodies.
//!
//! The selected joint is ringed in the look's accent, and a powered motor draws a
//! direction arrow (curved for hinges, straight for sliders) whose
//! length scales with target velocity — the "visualize motor state" the
//! joint feedback asked for.
//!
//! **Load colour** (brief N-9): each joint's face tints from its calm
//! colour to the look's `danger` ink as its reaction force nears the force
//! it is rated for ([`JointLoad`], fully hot at [`HOT_LOAD`]), so a person
//! sees which joint will fail. The load is derived render state: measured
//! every frame from `physics::queries`, never saved, never undone.

use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_core::units::PosRot;
use avian2d::prelude::RigidBody;
use gradiance_domain::appearance::Rgba;
use gradiance_domain::joint::{
    AngularMotorDef, HOT_LOAD, JointDef, JointKind, LinearMotorDef, load_fraction,
};
use gradiance_domain::settings::{RenderSettings, SimSettings};
use gradiance_physics::queries::PhysicsQueries;
use gradiance_domain::theme::active;
use gradiance_interaction::joint_edit::{
    HINGE_LIMIT_RADIUS_PX, HINGE_RING_PX, JointLimitDrag, slider_span,
};
use gradiance_interaction::overlay::OverlayGizmos;
use gradiance_interaction::selection::SelectedJoint;

/// A joint's load: its reaction force as a fraction of the force it is
/// rated for ([`load_fraction`]). **Derived** render state on every joint
/// entity (a required component of `JointDef`), rewritten each frame by
/// [`measure_joint_loads`]; never saved, never captured by undo. Zero for a
/// broken joint and before the first solve.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct JointLoad(pub f32);

/// The face colour of a joint glyph at `load`: `calm` at no load, the
/// look's `hot` ink from [`HOT_LOAD`] up, a straight blend between. Pure.
#[must_use]
pub fn load_colour(calm: Rgba, hot: Rgba, load: f32) -> Rgba {
    calm.lerp(hot, load / HOT_LOAD)
}

/// Measures every joint's [`JointLoad`] from avian's post-solve reaction
/// force, read through `physics::queries::joint_force` (a broken joint
/// reads zero), against its break force or, for an unbreakable joint, the
/// default rating for the weight of its moving bodies.
///
/// Cost: `O(joints)` per frame and allocation-free: per joint one force
/// read, at most two id lookups, and the mass and body kind of each
/// connected body. Paused, the last solve's load stays shown.
pub fn measure_joint_loads(
    mut joints: Query<(Entity, &JointDef, &mut JointLoad)>,
    physics: PhysicsQueries,
    index: Res<IdIndex>,
    kinds: Query<&RigidBody>,
    sim: Res<SimSettings>,
) {
    let gravity = sim.gravity.value().length();
    for (entity, def, mut load) in &mut joints {
        let force = physics.joint_force(entity).unwrap_or_default();
        let moving_mass: f32 = def
            .referenced_bodies()
            .filter_map(|id| index.entity(id))
            .filter(|&e| kinds.get(e).is_ok_and(RigidBody::is_dynamic))
            .filter_map(|e| physics.mass_of(e))
            .map(gradiance_units::Mass::value)
            .sum();
        let next = JointLoad(load_fraction(
            force,
            def.common.break_force,
            moving_mass,
            gravity,
        ));
        load.set_if_neq(next);
    }
}

/// Installs the load measurement: derived-state upkeep, so it runs headless
/// too (tests read [`JointLoad`]); drawing reads it.
pub fn install_load(app: &mut App) {
    app.register_required_components::<JointDef, JointLoad>();
    app.add_systems(Update, measure_joint_loads);
}

/// Draws every joint's anchor glyph (hinge = ring, weld = square,
/// slider = axis line), following the connected bodies.
pub fn draw_joints(
    joints: Query<(Entity, &JointDef, Option<&JointLoad>)>,
    selected: Res<SelectedJoint>,
    limit_drag: Res<JointLimitDrag>,
    index: Res<IdIndex>,
    transforms: Query<&Transform>,
    cam_scale: Res<gradiance_interaction::camera::CameraScale>,
    settings: Option<Res<RenderSettings>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    // Screen-constant glyph size: readable at any zoom.
    let s = cam_scale.0;
    let ink = &active(settings.as_deref()).ink;
    let outline = Color::from(ink.joint_outline);
    let accent = Color::from(ink.accent);
    let snap = Color::from(ink.snap);
    for (entity, def, load) in &joints {
        let load = load.map_or(0.0, |l| l.0);
        let Some(entity_a) = index.entity(def.body_a) else {
            continue;
        };
        let Ok(pose_a) = transforms.get(entity_a).map(PosRot::from_transform) else {
            continue;
        };
        let anchor = def.anchor_world(pose_a.pos, pose_a.rot);
        if def.common.broken {
            // A broken joint constrains nothing, so none of its kind glyph,
            // limits, or motor applies: draw a greyed dashed ring on each
            // half's anchor, which drift apart as the bodies separate.
            let world_b = anchor_b_world(def, &index, &transforms);
            for end in std::iter::once(anchor).chain(world_b) {
                draw_dashed_ring(&mut gizmos, end, HINGE_RING_PX * s, ink.muted.into());
            }
            if selected.0 == Some(entity) {
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 11.0 * s, accent);
            }
            continue;
        }
        // Light glyphs with a dark under-stroke for contrast on any body
        // color (feedback 4.7). The face carries the load, calm to hot; a
        // world pin keeps its warning hue as an outer ring.
        let pin = def.body_b.is_none();
        let color = Color::from(load_colour(ink.joint, ink.danger, load));
        let pin_ring = Color::from(ink.danger);
        match &def.kind {
            JointKind::Hinge { motor, limits } => {
                // Under-stroke: a slightly offset dark ring reads as an
                // outline at every zoom (gizmo lines have no width knob).
                let r = HINGE_RING_PX * s;
                gizmos.circle_2d(Isometry2d::from_translation(anchor), r * 1.1, outline);
                if pin {
                    gizmos.circle_2d(Isometry2d::from_translation(anchor), r * 1.4, pin_ring);
                }
                gizmos.circle_2d(Isometry2d::from_translation(anchor), r, color);
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 3.0 * s, color);
                // Exact allowed-rotation range (relative to body A), with
                // grab handles at the ends when the joint is selected. A
                // live handle drag previews its tentative range instead.
                let (limits, live) = tentative_or(
                    &limit_drag,
                    entity,
                    limits.map(|[lo, hi]| [lo.value(), hi.value()]),
                );
                if let Some([min, max]) = limits {
                    let arc_color = Color::from(if live { ink.accent } else { ink.guide });
                    draw_limit_arc(
                        &mut gizmos,
                        anchor,
                        def.limit_reference_angle(pose_a.rot),
                        [min, max],
                        s,
                        arc_color,
                        (selected.0 == Some(entity)).then_some(snap),
                    );
                }
                if let Some(m) = motor {
                    draw_angular_motor(&mut gizmos, anchor, *m, s, snap);
                }
            }
            JointKind::Slider { motor, limits, .. } => {
                // The travel is measured from body A's anchor, or on a world
                // pin from the pin (`JointDef::slider_frame`), where the
                // engine measures it.
                let Some((origin, dir)) = def.slider_frame(anchor, pose_a.rot) else {
                    continue;
                };
                // With travel limits the axis line shows the exact allowed
                // travel (plus end caps); unlimited prismatics keep the
                // long reference line. A live handle drag previews its
                // tentative travel instead.
                let (limits, live) = tentative_or(
                    &limit_drag,
                    entity,
                    limits.map(|[lo, hi]| [lo.value(), hi.value()]),
                );
                let span_color = if live { accent } else { color };
                let (from, to) = slider_span(origin, dir, limits, s);
                let perp = Vec2::new(-dir.y, dir.x) * 4.0 * s;
                gizmos.line_2d(from, to, span_color);
                if limits.is_some() {
                    gizmos.line_2d(from - perp, from + perp, span_color);
                    gizmos.line_2d(to - perp, to + perp, span_color);
                    if selected.0 == Some(entity) {
                        // Grab handles on the travel caps.
                        gizmos.circle_2d(Isometry2d::from_translation(from), 4.0 * s, snap);
                        gizmos.circle_2d(Isometry2d::from_translation(to), 4.0 * s, snap);
                    }
                }
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 6.6 * s, outline);
                if pin {
                    gizmos.circle_2d(Isometry2d::from_translation(anchor), 8.4 * s, pin_ring);
                }
                gizmos.circle_2d(Isometry2d::from_translation(anchor), 6.0 * s, color);
                if let Some(m) = motor {
                    draw_linear_motor(&mut gizmos, anchor, dir, *m, s, snap);
                }
            }
            JointKind::Spring { .. } => {
                // A coil between the two anchors; the connected bodies don't
                // collide, so it reads as a free spring, not a rod.
                if let Some(b) = anchor_b_world(def, &index, &transforms) {
                    let coil = Color::from(load_colour(ink.alternate, ink.danger, load));
                    draw_spring(&mut gizmos, anchor, b, s, coil);
                }
            }
        }

        // Selection highlight.
        if selected.0 == Some(entity) {
            gizmos.circle_2d(Isometry2d::from_translation(anchor), 11.0 * s, accent);
        }
    }
}

/// Body B's anchor in world space (a world pin's `anchor_b` already is one);
/// `None` while body B doesn't resolve.
fn anchor_b_world(def: &JointDef, index: &IdIndex, transforms: &Query<&Transform>) -> Option<Vec2> {
    match def.body_b {
        Some(id) => index
            .entity(id)
            .and_then(|e| transforms.get(e).ok())
            .map(PosRot::from_transform)
            .map(|pose_b| pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b.value())),
        None => Some(def.anchor_b.value()), // world pin
    }
}

/// A ring of `DASHES` short chords with gaps between them — the broken-joint
/// glyph (gizmos have no dash style).
fn draw_dashed_ring(gizmos: &mut Gizmos<OverlayGizmos>, center: Vec2, r: f32, color: Color) {
    const DASHES: u16 = 10;
    let step = std::f32::consts::TAU / f32::from(DASHES);
    for i in 0..DASHES {
        let a0 = f32::from(i) * step;
        let a1 = a0 + step * 0.55;
        gizmos.line_2d(
            center + Vec2::from_angle(a0) * r,
            center + Vec2::from_angle(a1) * r,
            color,
        );
    }
}

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
/// end ticks, and grab-handle dots in the given colour when `handles` is
/// set.
fn draw_limit_arc(
    gizmos: &mut Gizmos<OverlayGizmos>,
    anchor: Vec2,
    rot_a: f32,
    [min, max]: [f32; 2],
    s: f32,
    color: Color,
    handles: Option<Color>,
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
        if let Some(handle) = handles {
            gizmos.circle_2d(Isometry2d::from_translation(at(a)), 3.0 * s, handle);
        }
    }
}

/// A curved arrow around the hinge showing spin direction & strength.
fn draw_angular_motor(
    gizmos: &mut Gizmos<OverlayGizmos>,
    anchor: Vec2,
    motor: AngularMotorDef,
    s: f32,
    color: Color,
) {
    if !motor.enabled || motor.target_velocity.value().abs() < 1e-3 {
        return;
    }
    let radius = 14.0 * s;
    // Sweep grows with drive speed: direction *and* magnitude at a glance
    // (feedback 4.1 "visually indicate motor state").
    let sweep = motor.target_velocity.value().signum()
        * (1.2 + motor.target_velocity.value().abs().min(10.0) * 0.16); // 1.2..2.8 rad
    let steps = 10;
    let mut prev = anchor + Vec2::from_angle(0.0) * radius;
    for i in 1..=steps {
        let a = sweep * (i as f32 / steps as f32);
        let next = anchor + Vec2::from_angle(a) * radius;
        gizmos.line_2d(prev, next, color);
        prev = next;
    }
    // Arrowhead at the sweep end.
    let tip = anchor + Vec2::from_angle(sweep) * radius;
    let tangent = Vec2::from_angle(sweep + std::f32::consts::FRAC_PI_2) * sweep.signum();
    gizmos.line_2d(
        tip,
        tip - tangent * 5.0 * s + (anchor - tip).normalize() * 4.0 * s,
        color,
    );
    gizmos.line_2d(
        tip,
        tip - tangent * 5.0 * s - (anchor - tip).normalize() * 4.0 * s,
        color,
    );
}

/// Draws a spring coil between two world points. Straight lead-ins at each end,
/// a zigzag body in the middle, and a dot at each anchor. Amplitude is
/// screen-constant (scaled by `s`); the coil count is fixed.
fn draw_spring(gizmos: &mut Gizmos<OverlayGizmos>, a: Vec2, b: Vec2, s: f32, color: Color) {
    const COILS: usize = 8;
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
    motor: LinearMotorDef,
    s: f32,
    color: Color,
) {
    if !motor.enabled || motor.target_velocity.value().abs() < 1e-3 {
        return;
    }
    let d = dir * motor.target_velocity.value().signum();
    // Arrow length grows with drive speed (16..34 px at screen scale). The
    // speed→length map is in SI now: target_velocity is m/s (was px/s before
    // the flip), so the cap is 6 m/s and the gain 3 px per m/s — the SI flip
    // left this at the pixel-era `.min(600.0) * 0.03`, which never grew for a
    // real m/s slider, so the arrow read as a dead stub.
    let tip = anchor + d * (16.0 + motor.target_velocity.value().abs().min(6.0) * 3.0) * s;
    gizmos.line_2d(anchor, tip, color);
    let back = -d;
    let perp = Vec2::new(-d.y, d.x);
    gizmos.line_2d(tip, tip + (back + perp) * 6.0 * s, color);
    gizmos.line_2d(tip, tip + (back - perp) * 6.0 * s, color);
}
