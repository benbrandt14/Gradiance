//! Defaults that behave (brief 4b-4): every physical default does something
//! visible and sane on a body of any mass, and wild behaviour sits only at
//! the ends of an edit range.
//!
//! The tests state behaviour, not numbers: a period in a comfortable band and
//! equal across masses, a motor at speed within half a second, a bounce that
//! never gains height. The rigs run over a pebble (0.1 m box, 0.01 kg), a
//! 1 kg box and a truck (10 m box at density 10, 1000 kg). The pure scaling
//! is unit-tested in `gradiance_domain::tuning`.
//!
//! `sweep` (ignored) prints the table recorded in the brief's Outcome:
//! `cargo test --test it defaults::sweep -- --ignored --nocapture`.

// Test-only file: unwraps are the failure mechanism; the sweep prints.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::too_many_lines
)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::{
    AngularVelocity, ColliderDensity, GravityScale, LinearVelocity, Restitution, RigidBody,
};
use bevy::prelude::*;
use gradiance::domain::tuning;
use gradiance::prelude::*;
use gradiance::units::{Angle, Displacement, Force, Frequency, Length};

/// A body by edge `size` (m) and `density`: pebble, unit, truck.
const MASSES: [(f32, f32); 3] = [(0.1, 1.0), (1.0, 1.0), (10.0, 10.0)];
/// Frames per second of the harness clock.
const FPS: f32 = 60.0;

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, def: JointDef) -> StableId {
    let record = JointRecord {
        id: StableId::new(),
        def,
    };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn body(pos: Vec2, w: f32, h: f32, density: f32) -> BodyRecord {
    let mut rec = box_record(pos, w, h);
    rec.physics.density = ColliderDensity(density);
    rec
}

fn fixed(pos: Vec2, w: f32, h: f32) -> BodyRecord {
    let mut rec = box_record(pos, w, h);
    rec.physics.rigid_body = RigidBody::Static;
    rec
}

fn joint(kind: JointKind, a: StableId, b: Option<StableId>, aa: Vec2, ab: Vec2) -> JointDef {
    JointDef {
        kind,
        common: JointCommon::default(),
        body_a: a,
        body_b: b,
        anchor_a: Displacement(aa),
        anchor_b: Displacement(ab),
        rest_rot_a: Angle(0.0),
        rest_rot_b: Angle(0.0),
    }
}

fn pos(app: &App, id: StableId) -> Vec2 {
    let e = entity_of(app, id).unwrap();
    app.world()
        .get::<Transform>(e)
        .unwrap()
        .translation
        .truncate()
}

fn omega(app: &App, id: StableId) -> f32 {
    let e = entity_of(app, id).unwrap();
    app.world().get::<AngularVelocity>(e).map_or(0.0, |w| w.0)
}

fn edit(app: &mut App, id: StableId, old: PropertyValue, new: PropertyValue) {
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange { id, old, new }],
    });
    app.update();
}

fn spring(frequency: f32, ratio: f32, rest: f32) -> JointKind {
    JointKind::Spring {
        rest_length: Length(rest),
        frequency: Frequency(frequency),
        damping_ratio: ratio,
        range: None,
    }
}

// ---------- spring rig ----------------------------------------------------

/// What a hung spring did: its period, its first and last swing amplitudes
/// (as a fraction of the body size) and whether it stayed finite and near.
#[derive(Debug)]
struct Bounce {
    period: f32,
    first: f32,
    last: f32,
    sane: bool,
}

/// A box of `size`/`density` hung `2·size` below a world pin by a spring at
/// its rest length, released, run `secs`. With `densify`, the density is
/// multiplied by 100 after the spring exists (the retune path).
fn hung_spring(size: f32, density: f32, f: f32, zeta: f32, secs: f32, densify: bool) -> Bounce {
    let mut app = headless_app();
    let id = spawn_body(
        &mut app,
        body(Vec2::new(0.0, -2.0 * size), size, size, density),
    );
    spawn_joint(
        &mut app,
        joint(
            spring(f, zeta, 2.0 * size),
            id,
            None,
            Vec2::ZERO,
            Vec2::ZERO,
        ),
    );
    if densify {
        edit(
            &mut app,
            id,
            PropertyValue::Density(ColliderDensity(density)),
            PropertyValue::Density(ColliderDensity(density * 100.0)),
        );
    }
    let mut ys = Vec::new();
    let mut sane = true;
    for _ in 0..(secs * FPS) as usize {
        step(&mut app, 1);
        let p = pos(&app, id);
        sane &= p.is_finite() && p.length() < 1000.0;
        ys.push(p.y / size);
    }
    let (period, first, last) = oscillation(&ys);
    Bounce {
        period,
        first,
        last,
        sane,
    }
}

/// Period (mean spacing of upward crossings of the settled mean), first-second
/// and last-second half-amplitudes of a sampled signal.
fn oscillation(ys: &[f32]) -> (f32, f32, f32) {
    let tail = &ys[ys.len() / 2..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    let crossings: Vec<usize> = (1..ys.len())
        .filter(|&i| ys[i - 1] < mean && ys[i] >= mean)
        .collect();
    let period = if crossings.len() >= 2 {
        (crossings[crossings.len() - 1] - crossings[0]) as f32 / (crossings.len() - 1) as f32 / FPS
    } else {
        f32::NAN
    };
    let amp = |w: &[f32]| {
        let hi = w.iter().copied().fold(f32::MIN, f32::max);
        let lo = w.iter().copied().fold(f32::MAX, f32::min);
        (hi - lo) * 0.5
    };
    let second = FPS as usize;
    (period, amp(&ys[..second]), amp(&ys[ys.len() - second..]))
}

#[test]
fn a_default_spring_bounces_with_one_period_at_any_mass() {
    let f = tuning::SPRING_FREQUENCY.default;
    let zeta = tuning::SPRING_DAMPING_RATIO.default;
    let periods: Vec<f32> = MASSES
        .iter()
        .map(|&(size, density)| {
            let b = hung_spring(size, density, f, zeta, 5.0, false);
            assert!(b.sane, "{size} m: {b:?}");
            // A visible, comfortable bounce: 0.3 to 2 s.
            assert!((0.3..=2.0).contains(&b.period), "{size} m: {b:?}");
            // It settles: the last second swings under a fifth of the first.
            assert!(b.last < 0.2 * b.first, "{size} m does not settle: {b:?}");
            b.period
        })
        .collect();
    let (lo, hi) = (
        periods.iter().copied().fold(f32::MAX, f32::min),
        periods.iter().copied().fold(f32::MIN, f32::max),
    );
    assert!(
        hi / lo < 1.05,
        "one period from a pebble to a truck: {periods:?}"
    );
}

#[test]
fn a_spring_keeps_its_period_when_its_body_gets_denser() {
    let before = hung_spring(1.0, 1.0, 1.5, 0.05, 3.0, false);
    let after = hung_spring(1.0, 1.0, 1.5, 0.05, 3.0, true);
    assert!(before.sane && after.sane, "{before:?} {after:?}");
    assert!(
        (after.period / before.period - 1.0).abs() < 0.05,
        "density x100 keeps the period: {before:?} -> {after:?}"
    );
}

#[test]
fn springs_stay_sane_and_monotonic_across_the_inner_range() {
    // Inner 10-90% of the frequency range, at the inner ratio ends, on the
    // lightest and heaviest body: finite, near, and a higher frequency
    // never gives a longer period.
    for &(size, density) in &[MASSES[0], MASSES[2]] {
        for t_zeta in [0.0_f32, 0.1] {
            let zeta = tuning::SPRING_DAMPING_RATIO.at(t_zeta);
            let mut last_period = f32::MAX;
            for t in [0.5_f32, 0.75, 0.9] {
                let f = tuning::SPRING_FREQUENCY.at(t);
                let b = hung_spring(size, density, f, zeta, 2.0, false);
                assert!(b.sane, "{size} m, f {f}, zeta {zeta}: {b:?}");
                if b.period.is_finite() {
                    assert!(
                        b.period <= last_period * 1.05,
                        "{size} m: period rose with frequency ({f} Hz: {b:?})"
                    );
                    last_period = b.period;
                }
            }
            let soft = hung_spring(
                size,
                density,
                tuning::SPRING_FREQUENCY.at(0.1),
                zeta,
                2.0,
                false,
            );
            assert!(soft.sane, "{size} m soft spring: {soft:?}");
        }
    }
}

// ---------- motor rigs ----------------------------------------------------

/// How a motor's relative speed `rel` (rad/s or m/s) tracked `target`.
#[derive(Debug)]
struct Drive {
    t90: f32,
    peak: f32,
    end: f32,
}

fn drive(samples: &[f32], target: f32) -> Drive {
    let t90 = samples
        .iter()
        .position(|v| *v >= 0.9 * target)
        .map_or(f32::INFINITY, |i| (i + 1) as f32 / FPS);
    Drive {
        t90,
        peak: samples.iter().copied().fold(f32::MIN, f32::max),
        end: samples[samples.len() - 1],
    }
}

/// A box wheel driven by a default hinge motor at its centre: pinned to the
/// world (`ground = false`) or to a static ground body that is the joint's
/// body A. Returns the relative speed (B against A) per frame for `secs`.
fn hinge_wheel(size: f32, density: f32, ground: bool, secs: f32) -> Vec<f32> {
    let mut app = headless_app();
    let id = spawn_body(&mut app, body(Vec2::ZERO, size, size, density));
    let kind = JointKind::Hinge {
        limits: None,
        motor: Some(AngularMotorDef::default()),
    };
    let sign = if ground {
        let g = spawn_body(&mut app, fixed(Vec2::new(0.0, -size), 20.0 * size, size));
        spawn_joint(
            &mut app,
            joint(kind, g, Some(id), Vec2::new(0.0, size), Vec2::ZERO),
        );
        1.0
    } else {
        // A world pin: the pin is body B, so B against A is minus the wheel.
        spawn_joint(&mut app, joint(kind, id, None, Vec2::ZERO, Vec2::ZERO));
        -1.0
    };
    (0..(secs * FPS) as usize)
        .map(|_| {
            step(&mut app, 1);
            sign * omega(&app, id)
        })
        .collect()
}

#[test]
fn a_default_hinge_motor_reaches_its_speed_at_any_mass() {
    let target = gradiance::domain::joint::DEFAULT_MOTOR_ANGULAR_VELOCITY;
    for &(size, density) in &MASSES {
        for ground in [false, true] {
            let d = drive(&hinge_wheel(size, density, ground, 1.0), target);
            assert!(d.t90 <= 0.5, "{size} m (ground {ground}): {d:?}");
            assert!(d.peak <= 1.2 * target, "{size} m overshoots: {d:?}");
            assert!((d.end - target).abs() < 0.1 * target, "{size} m: {d:?}");
        }
    }
}

#[test]
fn a_default_hinge_motor_lifts_an_arm_of_a_metre_or_more() {
    // Gravity loads an arm pinned at one end. avian's motor corrects per
    // substep at the body's centre-of-mass inertia, so a short arm loses
    // speed to gravity (a 0.3 m arm holds ~75%, a 0.1 m one ~25%: see the
    // 4b-4 Outcome); from a metre up the default holds its speed.
    let target = gradiance::domain::joint::DEFAULT_MOTOR_ANGULAR_VELOCITY;
    for (size, density) in [(1.0_f32, 1.0_f32), (10.0, 10.0)] {
        let mut app = headless_app();
        let id = spawn_body(&mut app, body(Vec2::ZERO, size, size * 0.25, density));
        let a = Vec2::new(-0.5 * size, 0.0);
        spawn_joint(
            &mut app,
            joint(
                JointKind::Hinge {
                    limits: None,
                    motor: Some(AngularMotorDef::default()),
                },
                id,
                None,
                a,
                a,
            ),
        );
        let mut slowest = f32::MAX;
        for frame in 0..120 {
            step(&mut app, 1);
            if frame >= 30 {
                slowest = slowest.min(-omega(&app, id));
            }
        }
        assert!(
            slowest > 0.8 * target,
            "{size} m arm sags under gravity: slowest {slowest}"
        );
    }
}

#[test]
fn a_motor_ceiling_follows_a_density_change() {
    let mut app = headless_app();
    let id = spawn_body(&mut app, body(Vec2::ZERO, 1.0, 1.0, 1.0));
    let j = spawn_joint(
        &mut app,
        joint(
            JointKind::Hinge {
                limits: None,
                motor: Some(AngularMotorDef::default()),
            },
            id,
            None,
            Vec2::ZERO,
            Vec2::ZERO,
        ),
    );
    step(&mut app, 3);
    let ceiling = |app: &App| {
        let e = entity_of(app, j).unwrap();
        app.world()
            .get::<avian2d::prelude::RevoluteJoint>(e)
            .unwrap()
            .motor
            .max_torque
    };
    let before = ceiling(&app);
    edit(
        &mut app,
        id,
        PropertyValue::Density(ColliderDensity(1.0)),
        PropertyValue::Density(ColliderDensity(1000.0)),
    );
    step(&mut app, 3);
    let after = ceiling(&app);
    assert!(
        (after / before - 1000.0).abs() < 10.0,
        "auto ceiling scales with the inertia: {before} -> {after}"
    );
}

#[test]
fn a_default_slider_motor_reaches_its_speed_at_any_mass() {
    let target = gradiance::domain::joint::DEFAULT_MOTOR_LINEAR_VELOCITY;
    for &(size, density) in &MASSES {
        let mut app = headless_app();
        let id = spawn_body(&mut app, body(Vec2::ZERO, size, size, density));
        spawn_joint(
            &mut app,
            joint(
                JointKind::Slider {
                    axis: Vec2::X,
                    limits: None,
                    motor: Some(LinearMotorDef::default()),
                },
                id,
                None,
                Vec2::ZERO,
                Vec2::ZERO,
            ),
        );
        let e = entity_of(&app, id).unwrap();
        let vs: Vec<f32> = (0..60)
            .map(|_| {
                step(&mut app, 1);
                // The world pin is body B: B against A is minus the body.
                -app.world().get::<LinearVelocity>(e).map_or(0.0, |v| v.0.x)
            })
            .collect();
        let d = drive(&vs, target);
        assert!(d.t90 <= 0.5, "{size} m: {d:?}");
        assert!(d.peak <= 1.2 * target, "{size} m overshoots: {d:?}");
    }
}

// ---------- contact rigs --------------------------------------------------

/// Height ratio of the first bounce of a box dropped from two body heights
/// onto a static floor, both at restitution `e`; and the highest the box got
/// after its release, as a ratio of the drop (1 = no gain).
fn bounce(size: f32, density: f32, e: f32) -> (f32, f32) {
    let mut app = headless_app();
    let mut floor = fixed(Vec2::new(0.0, -size), 40.0 * size, size);
    floor.physics.restitution = Restitution::new(e);
    spawn_body(&mut app, floor);
    let drop = 2.0 * size;
    let mut rec = body(Vec2::new(0.0, drop), size, size, density);
    rec.physics.restitution = Restitution::new(e);
    let id = spawn_body(&mut app, rec);
    let rest_y = 0.0; // the floor's top (-size/2) plus half the box
    // Long enough to fall and come back up: four free-fall times, plus a
    // second.
    let fall = (2.0 * drop / 9.81).sqrt();
    let frames = ((4.0 * fall + 1.0) * FPS) as usize;
    let ys: Vec<f32> = (0..frames)
        .map(|_| {
            step(&mut app, 1);
            pos(&app, id).y
        })
        .collect();
    let contact = ys
        .iter()
        .position(|y| *y < rest_y + 0.02 * size)
        .unwrap_or(0);
    let apex = ys[contact..].iter().copied().fold(f32::MIN, f32::max);
    let highest = ys.iter().copied().fold(f32::MIN, f32::max);
    (
        (apex - rest_y) / (drop - rest_y),
        (highest - rest_y) / (drop - rest_y),
    )
}

#[test]
fn restitution_bounces_alike_at_any_mass_and_never_gains_height() {
    let mut last = vec![-1.0_f32; MASSES.len()];
    for e in [
        0.0_f32,
        tuning::RESTITUTION.default,
        0.7,
        tuning::RESTITUTION.max,
    ] {
        let ratios: Vec<f32> = MASSES
            .iter()
            .map(|&(size, density)| {
                let (ratio, highest) = bounce(size, density, e);
                assert!(highest <= 1.01, "{size} m gained height at e = {e}");
                ratio
            })
            .collect();
        for (i, r) in ratios.iter().enumerate() {
            assert!(
                *r >= last[i] - 0.01,
                "bounce fell as e rose to {e}: {ratios:?}"
            );
        }
        let spread = ratios.iter().copied().fold(f32::MIN, f32::max)
            - ratios.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            spread < 0.05,
            "mass changes the bounce at e = {e}: {ratios:?}"
        );
        last = ratios;
    }
    // The default bounces visibly, and the top of the range clearly more.
    let (default_ratio, _) = bounce(1.0, 1.0, tuning::RESTITUTION.default);
    assert!(
        default_ratio > 0.03,
        "default barely bounces: {default_ratio}"
    );
    assert!(
        last[1] > 0.5,
        "e = 1 should keep most of the height: {last:?}"
    );
}

/// How far (m) a box slid down a static slope of `deg` in 1 s.
fn slide(size: f32, density: f32, deg: f32, mu: f32) -> f32 {
    let mut app = headless_app();
    let angle = deg.to_radians();
    let dir = Vec2::from_angle(angle);
    let normal = dir.perp();
    let mut slope = fixed(Vec2::ZERO, 40.0 * size, size);
    slope.pose.rot = angle;
    slope.physics.friction = avian2d::prelude::Friction::new(mu);
    spawn_body(&mut app, slope);
    // A flat box (half as tall as long) so it slides before it tips.
    let start = normal * (0.75 * size + 0.001 * size);
    let mut rec = body(start, size, 0.5 * size, density);
    rec.pose.rot = angle;
    rec.physics.friction = avian2d::prelude::Friction::new(mu);
    let id = spawn_body(&mut app, rec);
    step(&mut app, 60);
    (start - pos(&app, id)).dot(dir)
}

#[test]
fn friction_holds_or_slides_alike_at_any_mass() {
    for &(size, density) in &MASSES {
        // The default holds on 25 degrees and slides on 35; the top of the
        // range holds on 40.
        let held = slide(size, density, 25.0, tuning::FRICTION.default);
        let slid = slide(size, density, 35.0, tuning::FRICTION.default);
        let grip = slide(size, density, 40.0, tuning::FRICTION.at(0.9));
        // Coulomb sliding: at least half the distance `a·t²/2` predicts,
        // whatever the mass.
        let (sin, cos) = 35_f32.to_radians().sin_cos();
        let predicted = 0.5 * 9.81 * (sin - tuning::FRICTION.default * cos);
        assert!(
            held.abs() < 0.05 * size,
            "{size} m slid on 25 deg: {held} m"
        );
        assert!(slid > 0.5 * predicted, "{size} m held on 35 deg: {slid} m");
        assert!(
            grip.abs() < 0.05 * size,
            "{size} m slid at high friction: {grip} m"
        );
    }
}

// ---------- break force -----------------------------------------------------

/// A pendulum (box on a breakable world-pin hinge at its end) released level
/// with the default break force for its weight; after a second, gravity is
/// scaled by `load`. Returns whether the joint broke before and after.
fn pendulum_breaks(size: f32, density: f32, load: f32) -> (bool, bool) {
    let mut app = headless_app();
    let rec = body(Vec2::ZERO, size, size * 0.25, density);
    let mass = size * size * 0.25 * density;
    let id = spawn_body(&mut app, rec);
    let a = Vec2::new(-0.5 * size, 0.0);
    let mut def = joint(
        JointKind::Hinge {
            limits: None,
            motor: None,
        },
        id,
        None,
        a,
        a,
    );
    def.common.break_force = Some(Force(tuning::default_break_force(mass, 9.81)));
    let j = spawn_joint(&mut app, def);
    let broken = |app: &App| {
        let e = entity_of(app, j).unwrap();
        app.world().get::<JointDef>(e).unwrap().common.broken
    };
    step(&mut app, 60);
    let before = broken(&app);
    edit(
        &mut app,
        id,
        PropertyValue::GravityScale(GravityScale(1.0)),
        PropertyValue::GravityScale(GravityScale(load)),
    );
    step(&mut app, 60);
    (before, broken(&app))
}

#[test]
fn a_default_break_force_holds_a_swing_and_breaks_under_a_heavy_load() {
    for &(size, density) in &[MASSES[0], MASSES[2]] {
        let (swing, loaded) = pendulum_breaks(size, density, 20.0);
        assert!(!swing, "{size} m: broke under its own swing");
        assert!(loaded, "{size} m: held twenty times its weight");
    }
}

// ---------- the sweep (Outcome table) --------------------------------------

#[test]
#[ignore = "sweep: prints the 4b-4 Outcome table"]
fn sweep() {
    let rigs = [
        (0.1_f32, 1.0_f32),
        (0.3, 1.0),
        (1.0, 1.0),
        (3.0, 1.0),
        (5.0, 4.0),
        (10.0, 10.0),
    ];
    println!(
        "\n| mass kg | spring period s (default) | first/last swing | motor t90 s (world, ground) | bounce e=0.3 | bounce e=1 |"
    );
    for (size, density) in rigs {
        let m = size * size * density;
        let b = hung_spring(size, density, 1.5, 0.1, 5.0, false);
        let w = drive(&hinge_wheel(size, density, false, 1.0), 2.0);
        let g = drive(&hinge_wheel(size, density, true, 1.0), 2.0);
        let (b3, _) = bounce(size, density, 0.3);
        let (b10, h10) = bounce(size, density, 1.0);
        println!(
            "| {m} | {:.3} | {:.3}/{:.3} | {:.3}, {:.3} (peak {:.2}, {:.2}) | {b3:.3} | {b10:.3} (max {h10:.3}) |",
            b.period, b.first, b.last, w.t90, g.t90, w.peak, g.peak
        );
    }
    println!("\n| f position | f Hz | period pebble | period truck | sane |");
    for t in [0.0_f32, 0.1, 0.5, 0.9, 1.0] {
        let f = tuning::SPRING_FREQUENCY.at(t);
        let p = hung_spring(0.1, 1.0, f, 0.1, 3.0, false);
        let tr = hung_spring(10.0, 10.0, f, 0.1, 3.0, false);
        println!(
            "| {t} | {f:.2} | {:.3} | {:.3} | {} |",
            p.period,
            tr.period,
            p.sane && tr.sane
        );
    }
    println!("\n| zeta | last/first swing (1 kg, 1.5 Hz, 5 s) |");
    for t in [0.0_f32, 0.05, 0.1, 0.5, 1.0] {
        let zeta = tuning::SPRING_DAMPING_RATIO.at(t);
        let b = hung_spring(1.0, 1.0, 1.5, zeta, 5.0, false);
        println!("| {zeta:.2} | {:.3} |", b.last / b.first.max(1e-9));
    }
    println!("\n| motor rate 1/s | arm 0.3 m slowest | arm 1 m slowest | wheel t90 |");
    for rate in [30.0_f32, 120.0, 360.0, 1000.0] {
        let mut row = Vec::new();
        for size in [0.3_f32, 1.0] {
            let mut app = headless_app();
            let id = spawn_body(&mut app, body(Vec2::ZERO, size, size * 0.25, 1.0));
            let a = Vec2::new(-0.5 * size, 0.0);
            let motor = AngularMotorDef {
                damping: rate,
                ..AngularMotorDef::default()
            };
            spawn_joint(
                &mut app,
                joint(
                    JointKind::Hinge {
                        limits: None,
                        motor: Some(motor),
                    },
                    id,
                    None,
                    a,
                    a,
                ),
            );
            let mut slowest = f32::MAX;
            for frame in 0..120 {
                step(&mut app, 1);
                if frame >= 30 {
                    slowest = slowest.min(-omega(&app, id));
                }
            }
            row.push(slowest);
        }
        let mut app = headless_app();
        let id = spawn_body(&mut app, body(Vec2::ZERO, 1.0, 1.0, 1.0));
        let motor = AngularMotorDef {
            damping: rate,
            ..AngularMotorDef::default()
        };
        spawn_joint(
            &mut app,
            joint(
                JointKind::Hinge {
                    limits: None,
                    motor: Some(motor),
                },
                id,
                None,
                Vec2::ZERO,
                Vec2::ZERO,
            ),
        );
        let ws: Vec<f32> = (0..60)
            .map(|_| {
                step(&mut app, 1);
                -omega(&app, id)
            })
            .collect();
        println!(
            "| {rate} | {:.2} | {:.2} | {:.3} |",
            row[0],
            row[1],
            drive(&ws, 2.0).t90
        );
    }
}
