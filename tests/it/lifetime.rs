//! Lifetime (brief S-7): a body that removes what it touches, a body that
//! removes itself after a time, and the `hit` contact pulse.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step, undo};
use avian2d::prelude::{ColliderDisabled, RigidBodyDisabled};
use bevy::prelude::*;
use gradiance::physics::lifetime::{ContactHits, Removed};
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
}

fn set_state(app: &mut App, state: GameState) {
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(state);
    app.update();
}

/// Every body, in id (spawn) order.
fn bodies(app: &mut App) -> Vec<Entity> {
    let mut query = app
        .world_mut()
        .query_filtered::<(Entity, &StableId), With<Body>>();
    let mut out: Vec<_> = query.iter(app.world()).map(|(e, id)| (e, *id)).collect();
    out.sort_by_key(|(_, id)| id.0);
    out.into_iter().map(|(e, _)| e).collect()
}

fn is_removed(app: &App, entity: Entity) -> bool {
    let world = app.world();
    let removed = world.get::<Removed>(entity).is_some();
    if removed {
        assert!(world.get::<ColliderDisabled>(entity).is_some());
        assert!(world.get::<RigidBodyDisabled>(entity).is_some());
        assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Hidden));
    }
    removed
}

/// The brief's behaviour test: a ball dropped on a remove-on-contact floor
/// is hidden and stops colliding after the impact (it stays where it was
/// removed, it does not fall through); undo of the run brings it back.
#[test]
fn a_remove_on_contact_floor_removes_a_ball_and_undo_restores_it() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-box 0 0 4 0.2) (spawn-circle 0 1 0.2))",
    );
    // One snapshot per run: the edits name bodies of the run before.
    run(&mut app, "(begin (set-static 0 1) (set-lifetime 0 1 0))");
    let [floor, ball] = bodies(&mut app)[..] else {
        panic!("two bodies");
    };
    let start = app.world().get::<Transform>(ball).unwrap().translation;
    set_state(&mut app, GameState::Playing);
    let mut hit = false;
    for _ in 0..120 {
        step(&mut app, 1);
        hit |= app.world().resource::<ContactHits>().0.contains(&ball);
        if is_removed(&app, ball) {
            break;
        }
    }
    assert!(hit, "the impact is a hit");
    assert!(is_removed(&app, ball), "the floor removed the ball");
    assert!(!is_removed(&app, floor), "the floor stays");
    let removed_at = app.world().get::<Transform>(ball).unwrap().translation;
    step(&mut app, 30);
    let later = app.world().get::<Transform>(ball).unwrap().translation;
    assert!(
        (later - removed_at).length() < 1e-4,
        "a removed ball does not move: {removed_at} -> {later}"
    );
    set_state(&mut app, GameState::Paused);
    undo(&mut app);
    app.update();
    assert!(!is_removed(&app, ball), "undo of the run brings it back");
    assert_eq!(
        app.world().get::<Visibility>(ball),
        Some(&Visibility::Inherited)
    );
    let back = app.world().get::<Transform>(ball).unwrap().translation;
    assert!((back - start).length() < 1e-4, "at its start: {back}");
}

/// A body with a 1 s time to live is gone at 1.0 s, give or take one step.
#[test]
fn a_one_second_ttl_body_is_gone_at_one_second() {
    let mut app = paused_app();
    run(&mut app, "(spawn-circle 0 5 0.2)");
    run(&mut app, "(set-lifetime 0 0 1)");
    let [ball] = bodies(&mut app)[..] else {
        panic!("one body");
    };
    set_state(&mut app, GameState::Playing);
    let mut frames = 0;
    while !is_removed(&app, ball) && frames < 200 {
        step(&mut app, 1);
        frames += 1;
    }
    assert!(
        (59..=61).contains(&frames),
        "removed at 1 s: after {frames} frames of 1/60 s"
    );
}

/// The `hit` quantity reads 1 on the step a contact starts and 0 later.
#[test]
fn the_hit_signal_pulses_on_impact() {
    let mut app = paused_app();
    run(
        &mut app,
        "(begin (spawn-ground 0 0 0) (spawn-circle 0 0.5 0.2))",
    );
    let ball = bodies(&mut app)[1];
    set_state(&mut app, GameState::Playing);
    let mut pulses = 0;
    for _ in 0..120 {
        step(&mut app, 1);
        pulses += usize::from(app.world().resource::<ContactHits>().0.contains(&ball));
    }
    assert!(pulses >= 1, "the landing is a hit");
    step(&mut app, 60);
    assert!(
        !app.world().resource::<ContactHits>().0.contains(&ball),
        "a resting ball does not pulse"
    );
}
