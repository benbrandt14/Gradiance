//! Baseline sweep (temporary, pre-change measurements).
#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::print_stdout)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::{AngularVelocity, ComputedAngularInertia, ComputedMass, LinearVelocity, RigidBody};
use bevy::prelude::*;
use gradiance::prelude::*;

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, def: JointDef) -> StableId {
    let record = JointRecord { id: StableId::new(), def };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn pos(app: &App, id: StableId) -> Vec2 {
    app.world().get::<Transform>(entity_of(app, id).unwrap()).unwrap().translation.truncate()
}

fn joint(kind: JointKind, a: StableId, b: Option<StableId>, aa: Vec2, ab: Vec2) -> JointDef {
    JointDef {
        kind,
        common: JointCommon::default(),
        body_a: a,
        body_b: b,
        anchor_a: gradiance::units::Displacement(aa),
        anchor_b: gradiance::units::Displacement(ab),
        rest_rot_a: gradiance::units::Angle(0.0),
        rest_rot_b: gradiance::units::Angle(0.0),
    }
}

const RIGS: [(f32, f32); 6] = [(0.1, 1.0), (0.3, 1.0), (1.0, 1.0), (3.0, 1.0), (5.0, 4.0), (10.0, 10.0)];

#[test]
#[ignore = "sweep: prints a table"]
fn sweep_baseline() {
    println!("\n## spring (k = 100 * aabb area, damping 0), body hung from a world pin");
    for (s, rho) in RIGS {
        let mut app = headless_app();
        let mut rec = box_record(Vec2::new(0.0, -2.0 * s), s, s);
        rec.physics.density = avian2d::prelude::ColliderDensity(rho);
        let id = spawn_body(&mut app, rec);
        let k = 100.0 * s * s;
        spawn_joint(&mut app, joint(JointKind::Spring {
            rest_length: gradiance::units::Length(2.0 * s),
            stiffness: gradiance::units::Stiffness(k),
            damping: gradiance::units::Damping(0.0),
            range: None,
        }, id, None, Vec2::ZERO, Vec2::ZERO));
        let mut ys = Vec::new();
        for _ in 0..300 { step(&mut app, 1); ys.push(pos(&app, id).y); }
        let m = app.world().get::<ComputedMass>(entity_of(&app, id).unwrap()).unwrap().value();
        let (period, amp_first, amp_last) = oscillation(&ys);
        println!("s={s} rho={rho} m={m:.3} period={period:.3}s amp0={:.3}s amp_end={:.3}s max_sag={:.3}s",
            amp_first / s, amp_last / s, (-2.0 * s - ys.iter().copied().fold(f32::MAX, f32::min)) / s);
    }

    println!("\n## hinge motor (default), world pin at the body centre / arm end; body_a static ground");
    for (s, rho) in RIGS {
        for case in ["centre", "arm", "ground_a"] {
            let mut app = headless_app();
            let mut rec = box_record(Vec2::ZERO, s, s * 0.25);
            rec.physics.density = avian2d::prelude::ColliderDensity(rho);
            let id = spawn_body(&mut app, rec);
            let motor = Some(AngularMotorDef::default());
            let kind = JointKind::Hinge { limits: None, motor };
            match case {
                "centre" => { spawn_joint(&mut app, joint(kind, id, None, Vec2::ZERO, Vec2::ZERO)); }
                "arm" => { spawn_joint(&mut app, joint(kind, id, None, Vec2::new(-s * 0.5, 0.0), Vec2::new(-s * 0.5, 0.0))); }
                _ => {
                    let mut g = box_record(Vec2::new(0.0, -s), 20.0 * s, s);
                    g.physics.rigid_body = RigidBody::Static;
                    let gid = spawn_body(&mut app, g);
                    spawn_joint(&mut app, joint(kind, gid, Some(id), Vec2::new(0.0, s), Vec2::ZERO));
                }
            }
            let e = entity_of(&app, id).unwrap();
            let mut ws = Vec::new();
            for _ in 0..120 { step(&mut app, 1); ws.push(app.world().get::<AngularVelocity>(e).map_or(0.0, |w| w.0)); }
            let t90 = ws.iter().position(|w| *w >= 1.8).map_or(f32::NAN, |i| (i + 1) as f32 / 60.0);
            let wmax = ws.iter().copied().fold(f32::MIN, f32::max);
            let wend = *ws.last().unwrap();
            let inertia = app.world().get::<ComputedAngularInertia>(e).unwrap().value();
            println!("s={s} rho={rho} {case}: I={inertia:.3e} t90={t90:.3}s wmax={wmax:.2} wend={wend:.2}");
        }
    }

    println!("\n## slider motor (default 2 m/s), world pin, no gravity effect along x");
    for (s, rho) in RIGS {
        let mut app = headless_app();
        let mut rec = box_record(Vec2::ZERO, s, s);
        rec.physics.density = avian2d::prelude::ColliderDensity(rho);
        let id = spawn_body(&mut app, rec);
        spawn_joint(&mut app, joint(JointKind::Slider { axis: Vec2::X, limits: None, motor: Some(LinearMotorDef::default()) }, id, None, Vec2::ZERO, Vec2::ZERO));
        let e = entity_of(&app, id).unwrap();
        let mut vs = Vec::new();
        for _ in 0..60 { step(&mut app, 1); vs.push(app.world().get::<LinearVelocity>(e).map_or(0.0, |v| v.0.x)); }
        let t90 = vs.iter().position(|v| *v >= 1.8).map_or(f32::NAN, |i| (i + 1) as f32 / 60.0);
        println!("s={s} rho={rho}: t90={t90:.3}s vend={:.2} = {:.1} body lengths/s", vs.last().unwrap(), vs.last().unwrap() / s);
    }

    println!("\n## drop onto static ground from 2 body heights: bounce ratio");
    for (s, rho) in RIGS {
        for e in [0.0_f32, 0.3, 0.5, 0.8, 0.95, 1.0] {
            let mut app = headless_app();
            let mut g = box_record(Vec2::new(0.0, -s), 40.0 * s, s);
            g.physics.rigid_body = RigidBody::Static;
            g.physics.restitution = avian2d::prelude::Restitution::new(e);
            spawn_body(&mut app, g);
            let drop_h = 2.0 * s;
            let mut rec = box_record(Vec2::new(0.0, drop_h), s, s);
            rec.physics.density = avian2d::prelude::ColliderDensity(rho);
            rec.physics.restitution = avian2d::prelude::Restitution::new(e);
            let id = spawn_body(&mut app, rec);
            let floor_y = -s * 0.5 + s * 0.5; // ground top + half body
            let mut ys = Vec::new();
            for _ in 0..240 { step(&mut app, 1); ys.push(pos(&app, id).y); }
            // first bounce apex after first contact
            let first_contact = ys.iter().position(|y| *y < floor_y + 0.02 * s).unwrap_or(0);
            let apex = ys[first_contact..].iter().copied().take(120).fold(f32::MIN, f32::max);
            let ratio = (apex - floor_y) / (drop_h - floor_y);
            let max_y = ys.iter().copied().fold(f32::MIN, f32::max);
            println!("s={s} rho={rho} e={e}: bounce_ratio={ratio:.3} (e^2={:.3}) max_y/start={:.3}", e * e, (max_y - floor_y) / (drop_h - floor_y));
        }
    }
}

/// Period from upward zero crossings around the mean, and first/last half-amplitudes.
fn oscillation(ys: &[f32]) -> (f32, f32, f32) {
    let tail = &ys[ys.len() / 2..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    let mut crossings = Vec::new();
    for i in 1..ys.len() {
        if ys[i - 1] < mean && ys[i] >= mean { crossings.push(i); }
    }
    let period = if crossings.len() >= 2 {
        (crossings[crossings.len() - 1] - crossings[0]) as f32 / (crossings.len() - 1) as f32 / 60.0
    } else { f32::NAN };
    let amp = |w: &[f32]| (w.iter().copied().fold(f32::MIN, f32::max) - w.iter().copied().fold(f32::MAX, f32::min)) * 0.5;
    (period, amp(&ys[..60]), amp(&ys[ys.len() - 60..]))
}

#[test]
#[ignore = "debug"]
fn debug_centre_motor() {
    for s in [0.1_f32, 1.0] {
        let mut app = headless_app();
        let rec = box_record(Vec2::ZERO, s, s * 0.25);
        let id = spawn_body(&mut app, rec);
        let j = spawn_joint(&mut app, joint(JointKind::Hinge { limits: None, motor: Some(AngularMotorDef::default()) }, id, None, Vec2::ZERO, Vec2::ZERO));
        for f in 0..6 {
            step(&mut app, 1);
            let je = entity_of(&app, j).unwrap();
            let rj = app.world().get::<avian2d::prelude::RevoluteJoint>(je);
            let unres = app.world().get::<gradiance::physics::joint_sync::JointUnresolved>(je).is_some();
            let e = entity_of(&app, id).unwrap();
            let w = app.world().get::<AngularVelocity>(e).map(|w| w.0);
            let sleeping = app.world().get::<avian2d::prelude::Sleeping>(e).is_some();
            println!("s={s} f={f} unresolved={unres} sleeping={sleeping} w={w:?} motor={:?}", rj.map(|r| (r.motor.enabled, r.motor.target_velocity, r.motor.max_torque)));
        }
    }
}

#[test]
#[ignore = "debug"]
fn debug_offsets() {
    for s in [0.1_f32, 1.0, 10.0] {
        for off in [0.0_f32, 0.001, 0.01, 0.1, 0.5] {
            for swap in [false, true] {
                let mut app = headless_app();
                let rec = box_record(Vec2::ZERO, s, s);
                let id = spawn_body(&mut app, rec);
                let mut g = box_record(Vec2::new(0.0, 3.0 * s), s, s);
                g.physics.rigid_body = RigidBody::Static;
                let gid = spawn_body(&mut app, g);
                let a = Vec2::new(off * s, 0.0);
                let kind = JointKind::Hinge { limits: None, motor: Some(AngularMotorDef::default()) };
                if swap {
                    spawn_joint(&mut app, joint(kind, gid, Some(id), a - Vec2::new(0.0, 3.0 * s), a));
                } else {
                    spawn_joint(&mut app, joint(kind, id, Some(gid), a, a - Vec2::new(0.0, 3.0 * s)));
                }
                let e = entity_of(&app, id).unwrap();
                let mut ws = Vec::new();
                for _ in 0..120 { step(&mut app, 1); ws.push(app.world().get::<AngularVelocity>(e).unwrap().0); }
                println!("s={s} off={off} static_is_a={swap}: w@0.1s={:.3} w@0.5s={:.3} w@2s={:.3}", ws[5], ws[29], ws[119]);
            }
        }
    }
}

#[test]
#[ignore = "debug"]
fn debug_projection() {
    let mut app = headless_app();
    let id = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
    let j = spawn_joint(&mut app, joint(JointKind::Hinge { limits: None, motor: Some(AngularMotorDef::default()) }, id, None, Vec2::ZERO, Vec2::ZERO));
    for f in 0..4 {
        step(&mut app, 1);
        let e = entity_of(&app, id).unwrap();
        let je = entity_of(&app, j).unwrap();
        let has = app.world().get::<avian2d::prelude::XpbdVelocityProjection>(e).is_some();
        let pin = app.world().get::<gradiance::physics::joint_sync::PinAnchor>(je).map(|p| p.0);
        let pin_has = pin.map(|p| app.world().get::<avian2d::prelude::XpbdVelocityProjection>(p).is_some());
        println!("f={f} body_has_proj={has} pin={pin:?} pin_has={pin_has:?} w={}", app.world().get::<AngularVelocity>(e).unwrap().0);
    }
}

#[test]
#[ignore = "debug"]
fn debug_velocity_persists() {
    for motor in [None, Some(AngularMotorDef::default())] {
        let mut app = headless_app();
        let id = spawn_body(&mut app, box_record(Vec2::ZERO, 1.0, 1.0));
        spawn_joint(&mut app, joint(JointKind::Hinge { limits: None, motor }, id, None, Vec2::ZERO, Vec2::ZERO));
        step(&mut app, 3);
        let e = entity_of(&app, id).unwrap();
        app.world_mut().get_mut::<AngularVelocity>(e).unwrap().0 = 5.0;
        for f in 0..4 {
            step(&mut app, 1);
            println!("motor={} f={f} w={} rot={}", motor.is_some(), app.world().get::<AngularVelocity>(e).unwrap().0, app.world().get::<avian2d::prelude::Rotation>(e).unwrap().as_radians());
        }
        let subs = app.world().resource::<avian2d::prelude::SubstepCount>().0;
        println!("substeps={subs}");
    }
}

#[test]
#[ignore = "debug"]
fn debug_damping_sweep() {
    for damping in [0.1_f32, 0.3, 0.6, 1.0, 1.5, 3.0, 10.0, 30.0] {
        for (s, case) in [(0.1_f32, "centre"), (1.0, "centre"), (10.0, "centre"), (0.1, "arm"), (1.0, "arm"), (10.0, "arm")] {
            let mut app = headless_app();
            let id = spawn_body(&mut app, box_record(Vec2::ZERO, s, s * 0.25));
            let m = AngularMotorDef { damping, ..AngularMotorDef::default() };
            let a = if case == "arm" { Vec2::new(-0.5 * s, 0.0) } else { Vec2::ZERO };
            spawn_joint(&mut app, joint(JointKind::Hinge { limits: None, motor: Some(m) }, id, None, a, a));
            let e = entity_of(&app, id).unwrap();
            let mut ws = Vec::new();
            for _ in 0..120 { step(&mut app, 1); ws.push(-app.world().get::<AngularVelocity>(e).unwrap().0); }
            let t90 = ws.iter().position(|w| *w >= 1.8).map_or(f32::NAN, |i| (i + 1) as f32 / 60.0);
            let mx = ws.iter().copied().fold(f32::MIN, f32::max);
            let mn = ws[60..].iter().copied().fold(f32::MAX, f32::min);
            println!("damping={damping} s={s} {case}: t90={t90:.3} max={mx:.2} min_after1s={mn:.2} end={:.2}", ws[119]);
        }
    }
}

#[test]
#[ignore = "debug"]
fn debug_small_arm() {
    for s in [0.03_f32, 0.1, 0.3] {
    for tau in [0.0_f32, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1, 1.0] {
        for damping in [0.3_f32, 0.6, 1.0] {
            let mut app = headless_app();
            let id = spawn_body(&mut app, box_record(Vec2::ZERO, s, s * 0.25));
            let m = AngularMotorDef { damping, max_torque: gradiance::units::Torque(tau), ..AngularMotorDef::default() };
            let a = Vec2::new(-0.5 * s, 0.0);
            spawn_joint(&mut app, joint(JointKind::Hinge { limits: None, motor: Some(m) }, id, None, a, a));
            let e = entity_of(&app, id).unwrap();
            let mut ws = Vec::new();
            let mut drift = 0.0_f32;
            for _ in 0..120 {
                step(&mut app, 1);
                ws.push(-app.world().get::<AngularVelocity>(e).unwrap().0);
                let t = app.world().get::<Transform>(e).unwrap();
                let end = t.translation.truncate() + Vec2::from_angle(t.rotation.to_euler(EulerRot::ZYX).0).rotate(a);
                drift = drift.max((end - a).length() / s);
            }
            let mx = ws.iter().copied().fold(f32::MIN, f32::max);
            let mn = ws[60..].iter().copied().fold(f32::MAX, f32::min);
            println!("s={s} tau={tau:e} gain={damping}: max={mx:.2} min_after1s={mn:.2} end={:.2} pivot_drift/s={drift:.3}", ws[119]);
        }
    }
    }
}
