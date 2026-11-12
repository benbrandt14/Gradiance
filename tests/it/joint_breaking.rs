//! Breaking limits: a joint with a `break_force` breaks during a run when its
//! reaction force exceeds the threshold; "broken" is authored state, so the
//! run boundary records it and undo restores the joint.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, paused_app, step, undo};
use avian2d::prelude::JointDisabled;
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::scene::{from_ron, to_ron};
use gradiance::signal::{SignalBinding, SignalBindings, SignalBus, SignalSink, SignalSource};
use gradiance::units::Force;

/// Where the test body hangs from its world pin (m).
const PIN: Vec2 = Vec2::new(0.0, 2.0);

const HINGE: JointKind = JointKind::Hinge {
    limits: None,
    motor: None,
};

fn spawn_body(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let record = box_record(pos, w, h);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn spawn_joint(app: &mut App, common: JointCommon, a: StableId, anchor: Vec2) -> StableId {
    let record = JointRecord {
        id: StableId::new(),
        def: JointDef {
            kind: HINGE,
            common,
            body_a: a,
            body_b: None,
            anchor_a: Vec2::ZERO,
            anchor_b: anchor,
            rest_rot_a: 0.0,
            rest_rot_b: 0.0,
        },
    };
    let id = record.id;
    app.world_mut().write_message(SpawnJointIntent { record });
    app.update();
    id
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

fn def_of(app: &App, id: StableId) -> JointDef {
    let entity = entity_of(app, id).unwrap();
    app.world().get::<JointDef>(entity).unwrap().clone()
}

/// The brief's unverified fact, pinned: a write to an authored joint
/// component *during play* (the way a physics system writes it — directly,
/// not through a command) is captured by the run boundary, so pausing
/// records it as the run's undo step and undoing restores the pre-run value.
#[test]
fn a_play_time_write_to_a_joint_def_is_captured_by_the_run_boundary() {
    let mut app = paused_app();
    let body = spawn_body(&mut app, Vec2::new(0.0, 2.0), 0.4, 0.4);
    let joint = spawn_joint(&mut app, JointCommon::default(), body, Vec2::new(0.0, 2.0));
    assert!(!def_of(&app, joint).common.collide_connected);

    set_state(&mut app, GameState::Playing);
    step(&mut app, 5);
    let entity = entity_of(&app, joint).unwrap();
    app.world_mut()
        .get_mut::<JointDef>(entity)
        .unwrap()
        .common
        .collide_connected = true;
    step(&mut app, 5);
    set_state(&mut app, GameState::Paused);
    assert!(
        def_of(&app, joint).common.collide_connected,
        "the play-time write survives the pause"
    );

    undo(&mut app);
    assert!(
        !def_of(&app, joint).common.collide_connected,
        "undoing the run restores the pre-run joint"
    );
}

/// A 1 m unit-density box (1 kg, ~10 N weight) hung by its centre from a
/// world-pin hinge with `break_force`. Returns `(body, joint)`.
fn hung_box(app: &mut App, break_force: Option<f32>) -> (StableId, StableId) {
    let body = spawn_body(app, PIN, 1.0, 1.0);
    let common = JointCommon {
        break_force: break_force.map(Force),
        ..JointCommon::default()
    };
    let joint = spawn_joint(app, common, body, PIN);
    (body, joint)
}

fn pos_of(app: &App, id: StableId) -> Vec2 {
    let entity = entity_of(app, id).unwrap();
    app.world()
        .get::<Transform>(entity)
        .unwrap()
        .translation
        .truncate()
}

fn is_disabled(app: &App, joint: StableId) -> bool {
    let entity = entity_of(app, joint).unwrap();
    app.world().get::<JointDisabled>(entity).is_some()
}

#[test]
fn an_overloaded_hinge_breaks_and_undo_restores_it() {
    let mut app = paused_app();
    // A 5 N limit under a ~10 N load.
    let (body, joint) = hung_box(&mut app, Some(5.0));

    set_state(&mut app, GameState::Playing);
    step(&mut app, 60);
    assert!(def_of(&app, joint).common.broken, "the hinge broke");
    assert!(is_disabled(&app, joint), "broken derives JointDisabled");
    let fallen = pos_of(&app, body);
    assert!(
        PIN.y - fallen.y > 1.0,
        "the body fell away from the pin: {fallen}"
    );

    set_state(&mut app, GameState::Paused);
    undo(&mut app);
    app.update(); // joint_sync re-derives the restored joint
    assert!(
        !def_of(&app, joint).common.broken,
        "undo restores it intact"
    );
    assert!(!is_disabled(&app, joint));
    assert!(
        (pos_of(&app, body) - PIN).length() < 0.05,
        "and the body is back on its pin"
    );
}

#[test]
fn an_unbreakable_hinge_holds_under_the_same_load() {
    let mut app = paused_app();
    let (body, joint) = hung_box(&mut app, None);

    set_state(&mut app, GameState::Playing);
    step(&mut app, 120);
    assert!(!def_of(&app, joint).common.broken);
    assert!(!is_disabled(&app, joint));
    assert!(
        (pos_of(&app, body) - PIN).length() < 0.05,
        "still hanging on its pin: {}",
        pos_of(&app, body)
    );
}

#[test]
fn the_joint_force_source_reads_the_load() {
    let mut app = paused_app();
    let (_, joint) = hung_box(&mut app, None);
    app.world_mut()
        .resource_mut::<SignalBindings>()
        .0
        .push(SignalBinding {
            name: "pull".into(),
            source: SignalSource::JointForce(joint),
            map: default(),
            curve: None,
            gradient: default(),
            sink: SignalSink::Plot,
        });
    set_state(&mut app, GameState::Playing);
    step(&mut app, 60);
    let pull = app.world().resource::<SignalBus>().get("pull").unwrap();
    // The pin carries the box's weight, m·g = 1 kg · 10 m/s².
    assert!((pull - 10.0).abs() < 1.0, "reaction ~ weight, read {pull}");
}

#[test]
fn a_broken_joint_stays_broken_through_save_and_load() {
    let mut app = paused_app();
    let (body, joint) = hung_box(&mut app, Some(5.0));
    set_state(&mut app, GameState::Playing);
    step(&mut app, 30);
    set_state(&mut app, GameState::Paused);
    assert!(def_of(&app, joint).common.broken);

    let text = to_ron(&SceneRecord::capture(app.world_mut())).unwrap();
    let scene = from_ron(&text).unwrap();

    let mut loaded = paused_app();
    loaded.world_mut().write_message(LoadSceneIntent { scene });
    step(&mut loaded, 3); // load + joint_sync resolution
    let def = def_of(&loaded, joint);
    assert!(def.common.broken, "the save kept it broken");
    assert_eq!(def.common.break_force, Some(Force(5.0)));
    assert!(is_disabled(&loaded, joint), "and it loads disconnected");

    // Running the loaded scene: nothing holds the body up.
    let before = pos_of(&loaded, body);
    set_state(&mut loaded, GameState::Playing);
    step(&mut loaded, 30);
    assert!(before.y - pos_of(&loaded, body).y > 0.5, "it falls freely");
}
