//! The laser node (brief N-8): a placeable node whose beam reflects off
//! bodies and publishes, under its name, `1` while the beam reaches its
//! target body (else `0`) and `<name>.distance`, the beam's path length. The
//! laser is authored (one undo step per edit, saved with the node); the beam
//! and the bus values are derived (never saved, never undone).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step, undo};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::command::intent::{DuplicateIntent, PropertyEditIntent, SpawnNodeIntent};
use gradiance::command::property::{PropertyChange, PropertyValue};
use gradiance::core::units::PosRot;
use gradiance::domain::appearance::Appearance;
use gradiance::domain::laser::Laser;
use gradiance::domain::node::{BehaviorNode, NodeAttachment, NodeKind};
use gradiance::domain::shape::ShapeDef;
use gradiance::op::invoke;
use gradiance::physics::drive::SignalDrive;
use gradiance::physics::laser::LaserBeam;
use gradiance::prelude::*;
use gradiance::scene::{NodeRecord, SceneRecord};
use gradiance::signal::{
    GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap, SignalSink, SignalSource,
};
use gradiance::units::{Acceleration2, Angle, Length};
use serde_json::json;
use std::f32::consts::{FRAC_PI_4, SQRT_2};

fn zero_gravity(app: &mut App) {
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Acceleration2(Vec2::ZERO);
    app.update();
}

fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

/// A static body (it neither falls nor is pushed).
fn fixed(mut record: BodyRecord) -> BodyRecord {
    record.physics.rigid_body = RigidBody::Static;
    record
}

/// A static disc of radius `r` at `pos`.
fn disc(pos: Vec2, r: f32) -> BodyRecord {
    let mut record = fixed(box_record(pos, 1.0, 1.0));
    record.shape = ShapeDef::Circle { radius: r };
    record
}

/// A free laser at `pos` aimed along the world `angle`, named `name`,
/// targeting `target`.
fn spawn_laser(
    app: &mut App,
    pos: Vec2,
    angle: f32,
    name: &str,
    target: Option<StableId>,
) -> StableId {
    let id = StableId::new();
    let record = NodeRecord {
        id,
        pose: PosRot { pos, rot: 0.0 },
        attachment: NodeAttachment::free(),
        kind: NodeKind::Laser(Laser {
            name: name.into(),
            target,
            angle: Angle(angle),
            ..Laser::default()
        }),
        appearance: Appearance::default(),
    };
    app.world_mut().write_message(SpawnNodeIntent { record });
    app.update();
    id
}

fn beam(app: &App, laser: StableId) -> LaserBeam {
    app.world()
        .get::<LaserBeam>(entity_of(app, laser).unwrap())
        .expect("a laser has a derived beam")
        .clone()
}

fn bus(app: &App, name: &str) -> Option<f32> {
    app.world().resource::<SignalBus>().get(name)
}

fn kind_of(app: &App, node: StableId) -> NodeKind {
    app.world()
        .get::<NodeKind>(entity_of(app, node).unwrap())
        .unwrap()
        .clone()
}

/// The mirror scene: a static floor whose top face is y = 0 (the mirror
/// wall), a laser at (0, 2) aimed down at 45 degrees, and a disc of radius
/// 0.25 placed by geometry on the reflected ray, at (4, 2).
fn mirror_scene(app: &mut App) -> (StableId, StableId) {
    zero_gravity(app);
    spawn_body(app, fixed(box_record(Vec2::new(0.0, -0.5), 20.0, 1.0)));
    let target = spawn_body(app, disc(Vec2::new(4.0, 2.0), 0.25));
    let laser = spawn_laser(app, Vec2::new(0.0, 2.0), -FRAC_PI_4, "gate", Some(target));
    step(app, 4);
    (laser, target)
}

/// The brief's behaviour test: a beam at 45 degrees onto a mirror wall
/// reflects and reaches the target body that geometry put on the reflected
/// ray; the bus reads 1 and the path length.
#[test]
fn a_beam_at_45_degrees_off_a_mirror_wall_hits_the_target() {
    let mut app = headless_app();
    let (laser, _) = mirror_scene(&mut app);
    let beam = beam(&app, laser);
    assert!(beam.hit, "the beam reaches the target: {beam:?}");
    assert_eq!(beam.path.len(), 3, "origin, the mirror, the target");
    assert!(
        (beam.path[1] - Vec2::new(2.0, 0.0)).length() < 1e-3,
        "it reflects where geometry says: {:?}",
        beam.path
    );
    let expected = 4.0 * SQRT_2 - 0.25;
    assert!(
        (beam.distance - expected).abs() < 1e-2,
        "distance {} vs {expected}",
        beam.distance
    );
    assert_eq!(bus(&app, "gate"), Some(1.0));
    let d = bus(&app, "gate.distance").expect("the distance is published");
    assert!((d - expected).abs() < 1e-2, "{d}");
}

/// Moved off the reflected ray, the target is missed: the bus reads 0, and
/// the beam runs on to its range.
#[test]
fn a_target_off_the_reflected_ray_reads_zero() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    spawn_body(&mut app, fixed(box_record(Vec2::new(0.0, -0.5), 20.0, 1.0)));
    let target = spawn_body(&mut app, disc(Vec2::new(4.0, 3.0), 0.25));
    let laser = spawn_laser(
        &mut app,
        Vec2::new(0.0, 2.0),
        -FRAC_PI_4,
        "gate",
        Some(target),
    );
    step(&mut app, 4);
    let beam = beam(&app, laser);
    assert!(!beam.hit);
    assert_eq!(bus(&app, "gate"), Some(0.0));
    let range = Laser::default().range.0;
    assert!(
        (beam.distance - range).abs() < 1e-2,
        "the beam runs to its range ({})",
        beam.distance
    );
}

/// A light gate: a body put across the beam blocks it, and the hit reads 0
/// until it is gone.
#[test]
fn a_body_across_the_beam_blocks_it() {
    let mut app = headless_app();
    let (laser, _) = mirror_scene(&mut app);
    assert_eq!(bus(&app, "gate"), Some(1.0));
    // Its bottom face crosses the reflected ray at (3, 1).
    let blocker = spawn_body(&mut app, fixed(box_record(Vec2::new(3.0, 1.2), 0.4, 0.4)));
    step(&mut app, 4);
    let blocked = beam(&app, laser);
    assert_eq!(bus(&app, "gate"), Some(0.0), "{blocked:?}");
    assert!(
        (blocked.path[2] - Vec2::new(3.0, 1.0)).length() < 1e-3,
        "the beam turns at the blocker: {:?}",
        blocked.path
    );
    app.world_mut()
        .write_message(gradiance::command::intent::DeleteIntent {
            targets: vec![blocker],
        });
    step(&mut app, 4);
    assert_eq!(bus(&app, "gate"), Some(1.0), "the gate opens again");
}

/// Two facing mirrors trap the beam: it stops at its bounce limit.
#[test]
fn the_bounce_limit_caps_the_path() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    spawn_body(&mut app, fixed(box_record(Vec2::new(-0.5, 0.0), 1.0, 40.0)));
    spawn_body(&mut app, fixed(box_record(Vec2::new(2.5, 0.0), 1.0, 40.0)));
    let laser = spawn_laser(&mut app, Vec2::new(1.0, 0.0), 0.1, "trap", None);
    step(&mut app, 4);
    let live = app
        .world()
        .get::<LaserBeam>(entity_of(&app, laser).unwrap())
        .unwrap();
    let bounces = Laser::default().bounces as usize;
    assert_eq!(live.path.len(), bounces + 2, "{:?}", live.path);
    assert!(!live.hit, "no target, no hit");
    assert_eq!(
        live.path.capacity(),
        gradiance::domain::laser::MAX_BOUNCES as usize + 2,
        "the path buffer is sized once for the cap and never grows"
    );
}

/// N-1 makes it a light gate: the laser's hit drives an actuator (here a
/// body's gravity scale) through a named binding, with no undo step.
#[test]
fn the_hit_signal_drives_an_actuator() {
    let mut app = headless_app();
    let (_, _) = mirror_scene(&mut app);
    let lift = spawn_body(&mut app, box_record(Vec2::new(-6.0, 4.0), 0.5, 0.5));
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "lift-gravity".into(),
            source: SignalSource::Named("gate".into()),
            map: SignalMap::default(),
            curve: None,
            gradient: GradientSpec::default(),
            sink: SignalSink::GravityScale(lift),
        });
    step(&mut app, 3);
    assert_eq!(
        app.world()
            .get::<SignalDrive>(entity_of(&app, lift).unwrap())
            .and_then(|d| d.gravity_scale),
        Some(1.0),
        "the gate's 1 drives the gravity scale"
    );
    // The binding itself is a settled settings edit; the beam adds nothing.
    step(&mut app, 30);
    let after = app.world().resource::<HistoryInfo>().undo_depth;
    assert!(after <= depth + 1, "the beam records no undo step");
}

/// A laser edit is one undo step; the beam follows the authored axis.
#[test]
fn a_laser_edit_is_one_undo_step() {
    let mut app = headless_app();
    let (laser, _) = mirror_scene(&mut app);
    let before = kind_of(&app, laser);
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    let NodeKind::Laser(mut edited) = before.clone() else {
        panic!("a laser");
    };
    // Straight up: nothing there.
    edited.angle = Angle(std::f32::consts::FRAC_PI_2);
    edited.range = Length(3.0);
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: laser,
            old: PropertyValue::NodeKind(before.clone()),
            new: PropertyValue::NodeKind(NodeKind::Laser(edited)),
        }],
    });
    step(&mut app, 4);
    assert_eq!(
        app.world().resource::<HistoryInfo>().undo_depth,
        depth + 1,
        "one edit, one step"
    );
    assert_eq!(bus(&app, "gate"), Some(0.0));
    assert!((beam(&app, laser).distance - 3.0).abs() < 1e-3);
    // Nothing in the scene moves, so the run leaves no "simulate" step: one
    // undo takes the edit back.
    undo(&mut app);
    assert_eq!(
        app.world().resource::<HistoryInfo>().redo_label,
        Some("property-edit")
    );
    assert_eq!(kind_of(&app, laser), before, "undo restores the laser");
    step(&mut app, 4);
    assert_eq!(bus(&app, "gate"), Some(1.0), "and the beam follows");
}

/// The laser is saved; the beam is not.
#[test]
fn the_laser_is_saved_and_the_beam_is_not() {
    let mut app = headless_app();
    let (laser, target) = mirror_scene(&mut app);
    let scene = SceneRecord::capture(app.world_mut());
    let node = scene
        .nodes
        .iter()
        .find(|n| n.id == laser)
        .expect("the laser is saved");
    let NodeKind::Laser(saved) = &node.kind else {
        panic!("a laser");
    };
    assert_eq!(saved.target, Some(target));
    assert_eq!(saved.name, "gate");
    let text = gradiance::scene::to_ron(&scene).expect("the scene serialises");
    assert!(!text.contains("LaserBeam") && !text.contains("hit_name"));
}

/// `(laser body x y angle target name)` places what the tool places: a free
/// laser with `-1` for the body, its target resolved, one undo step.
#[test]
fn the_laser_operation_places_a_targeted_laser() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    spawn_body(&mut app, fixed(box_record(Vec2::new(0.0, -0.5), 20.0, 1.0)));
    let target = spawn_body(&mut app, disc(Vec2::new(4.0, 2.0), 0.25));
    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    let handle = invoke(
        app.world_mut(),
        "node.laser",
        json!({
            "body": -1, "x": 0.0, "y": 2.0, "angle": -FRAC_PI_4,
            "target": target.0.to_string(), "name": "door"
        }),
    )
    .expect("the operation lowers");
    step(&mut app, 4);
    let id = handle
        .and_then(|v| v.as_str().map(str::to_owned))
        .expect("a handle");
    let (attachment, kind) = app
        .world_mut()
        .query_filtered::<(&StableId, &NodeAttachment, &NodeKind), With<BehaviorNode>>()
        .iter(app.world())
        .find(|(n, ..)| n.0.to_string() == id)
        .map(|(_, a, k)| (*a, k.clone()))
        .expect("the node exists");
    assert_eq!(attachment.target, None, "a free laser");
    let NodeKind::Laser(laser) = kind else {
        panic!("a laser");
    };
    assert_eq!(laser.target, Some(target));
    assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, depth + 1);
    assert_eq!(bus(&app, "door"), Some(1.0), "its beam reaches the target");
}

/// A laser riding a body turns with it, and a duplicated pair aims at its
/// own copy of the target.
#[test]
fn duplicating_a_laser_and_its_target_retargets_the_copy() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let base = spawn_body(&mut app, fixed(box_record(Vec2::ZERO, 0.5, 0.5)));
    let target = spawn_body(&mut app, disc(Vec2::new(3.0, 0.0), 0.25));
    let id = StableId::new();
    app.world_mut().write_message(SpawnNodeIntent {
        record: NodeRecord {
            id,
            pose: PosRot {
                pos: Vec2::new(0.3, 0.0),
                rot: 0.0,
            },
            attachment: NodeAttachment::to(base, Vec2::new(0.3, 0.0)),
            kind: NodeKind::Laser(Laser {
                target: Some(target),
                ..Laser::default()
            }),
            appearance: Appearance::default(),
        },
    });
    step(&mut app, 4);
    assert!(beam(&app, id).hit, "the beam leaves its own body and hits");
    app.world_mut().write_message(DuplicateIntent {
        sources: vec![base, target],
        offset: Vec2::new(0.0, 5.0),
    });
    step(&mut app, 4);
    let lasers: Vec<(StableId, Laser)> = app
        .world_mut()
        .query_filtered::<(&StableId, &NodeKind), With<BehaviorNode>>()
        .iter(app.world())
        .filter_map(|(n, k)| match k {
            NodeKind::Laser(l) => Some((*n, l.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(lasers.len(), 2);
    let (copy, laser) = lasers.into_iter().find(|(n, _)| *n != id).unwrap();
    assert!(
        laser.target.is_some_and(|t| t != target),
        "the copy aims at the copied target"
    );
    assert!(beam(&app, copy).hit, "and reaches it");
}

/// Brief S-3: a beam through a 60-degree prism of index 1.5, entering at
/// the minimum-deviation incidence, is turned by the textbook angle
/// `2 asin(n sin 30°) − 60° ≈ 37.18°`; the same prism as a mirror (no
/// index) reflects it instead.
#[test]
fn a_prism_deviates_a_beam_by_the_minimum_deviation_angle() {
    let n: f32 = 1.5;
    let apex = 60_f32.to_radians();
    let expected = 2.0 * (n * (apex / 2.0).sin()).asin() - apex;
    let mut app = headless_app();
    zero_gravity(&mut app);
    // An equilateral prism of side 2 m, centroid at the origin, base down.
    let h = 3_f32.sqrt();
    let mut prism = box_record(Vec2::ZERO, 1.0, 1.0);
    prism.shape = ShapeDef::Polygon {
        outline: vec![
            Vec2::new(-1.0, -h / 3.0),
            Vec2::new(1.0, -h / 3.0),
            Vec2::new(0.0, 2.0 * h / 3.0),
        ],
        holes: Vec::new(),
    };
    prism.physics.rigid_body = RigidBody::Static;
    let prism_id = spawn_body(&mut app, prism);
    invoke(
        app.world_mut(),
        "body.set_index",
        json!({"i": prism_id.0.to_string(), "n": n}),
    )
    .unwrap();
    // Aim at the left face's midpoint, at the incidence that runs the beam
    // parallel to the base inside: (apex + deviation) / 2 from the normal,
    // which is a rise of that minus 30 degrees.
    let incidence = f32::midpoint(apex, expected);
    let rise = incidence - apex / 2.0;
    let dir = Vec2::from_angle(rise);
    let mid = Vec2::new(-0.5, h / 6.0);
    let laser = spawn_laser(&mut app, mid - dir * 2.0, rise, "prism", None);
    step(&mut app, 4);
    let path = beam(&app, laser).path;
    assert!(path.len() >= 4, "into, through and out: {path:?}");
    let first = (path[1] - path[0]).normalize();
    let last = (path[path.len() - 1] - path[path.len() - 2]).normalize();
    let deviation = first.angle_to(last).abs().to_degrees();
    assert!(
        (deviation - expected.to_degrees()).abs() < 0.5,
        "deviated {deviation} degrees, expected {}",
        expected.to_degrees()
    );
    // Inside, the beam runs parallel to the base.
    let inside = (path[2] - path[1]).normalize();
    assert!(inside.y.abs() < 0.01, "inside: {inside}");

    // As a mirror it reflects at the first face.
    invoke(
        app.world_mut(),
        "body.set_index",
        json!({"i": prism_id.0.to_string(), "n": 0}),
    )
    .unwrap();
    step(&mut app, 4);
    let path = beam(&app, laser).path;
    let back = (path[2] - path[1]).normalize();
    assert!(back.x < 0.0, "reflected back left: {path:?}");
}

/// A white laser through a dispersive prism (brief S-15): every colour
/// shares the beam up to the prism, then the colours fan out, violet bent
/// most; a coloured laser through the same prism keeps one path.
#[test]
fn a_white_laser_through_a_prism_makes_a_spectrum() {
    let mut app = headless_app();
    zero_gravity(&mut app);
    let h = 3_f32.sqrt();
    let mut prism = box_record(Vec2::ZERO, 1.0, 1.0);
    prism.shape = ShapeDef::Polygon {
        outline: vec![
            Vec2::new(-1.0, -h / 3.0),
            Vec2::new(1.0, -h / 3.0),
            Vec2::new(0.0, 2.0 * h / 3.0),
        ],
        holes: Vec::new(),
    };
    prism.physics.rigid_body = RigidBody::Static;
    let prism_id = spawn_body(&mut app, prism);
    invoke(
        app.world_mut(),
        "body.set_dispersion",
        json!({"i": prism_id.0.to_string(), "b": 0.01}),
    )
    .unwrap();
    let rise = 10_f32.to_radians();
    let dir = Vec2::from_angle(rise);
    let laser = spawn_laser(
        &mut app,
        Vec2::new(-0.5, h / 6.0) - dir * 2.0,
        rise,
        "rainbow",
        None,
    );
    step(&mut app, 2);
    assert!(beam(&app, laser).spectrum.is_empty(), "a coloured laser");
    let before = kind_of(&app, laser);
    let NodeKind::Laser(mut white) = before.clone() else {
        panic!("a laser");
    };
    white.white = true;
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id: laser,
            old: PropertyValue::NodeKind(before),
            new: PropertyValue::NodeKind(NodeKind::Laser(white)),
        }],
    });
    step(&mut app, 2);
    let beam = beam(&app, laser);
    assert_eq!(beam.spectrum.len(), 7);
    assert_eq!(beam.split, 2, "white to the prism: {:?}", beam.path);
    let heading = |path: &[Vec2]| {
        let n = path.len();
        let d = path[n - 1] - path[n - 2];
        d.y.atan2(d.x).to_degrees()
    };
    let red = heading(&beam.spectrum[0]);
    let violet = heading(&beam.spectrum[6]);
    assert!(
        red - violet > 0.5,
        "violet bends most: red {red}, violet {violet}"
    );
}
