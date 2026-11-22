//! Liquify, water fill and drain (brief S-11): one undo step each; the
//! particles are derived and fill when the run plays.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::domain::node::NodeKind;
use gradiance::physics::particles::ParticleFluid;
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

/// Every body as `(id, shape, transform)`, in id order.
fn bodies(app: &mut App) -> Vec<(StableId, ShapeDef, Transform)> {
    let mut query = app
        .world_mut()
        .query_filtered::<(&StableId, &ShapeDef, &Transform), With<Body>>();
    let mut out: Vec<_> = query
        .iter(app.world())
        .map(|(id, s, t)| (*id, s.clone(), *t))
        .collect();
    out.sort_by_key(|(id, _, _)| id.0);
    out
}

/// The water emitters in the scene.
fn emitters(app: &mut App) -> usize {
    let mut query = app.world_mut().query::<&NodeKind>();
    query
        .iter(app.world())
        .filter(|k| matches!(k, NodeKind::Water(_)))
        .count()
}

/// The particle area: count times spacing squared.
fn particle_area(app: &App) -> f32 {
    let fluid = app.world().resource::<ParticleFluid>();
    fluid.particles.len() as f32 * fluid.params.spacing.powi(2)
}

/// Liquify keeps the area within 5 %; undo brings the body back exactly.
#[test]
fn liquify_keeps_the_area_and_undo_restores_the_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box-rot 0.3 1 1 0.6 0.4)");
    let before = bodies(&mut app);
    assert_eq!(before.len(), 1);
    run(&mut app, "(liquify 0)");
    assert!(bodies(&mut app).is_empty(), "the body goes");
    assert_eq!(emitters(&mut app), 1, "a water emitter takes its place");
    set_state(&mut app, GameState::Playing);
    step(&mut app, 2);
    let area = particle_area(&app);
    assert!(
        (area - 0.6).abs() <= 0.6 * 0.05,
        "particle area {area} m2 against the body's 0.6 m2"
    );
    set_state(&mut app, GameState::Paused);
    // The run moved no body, so it is no history step: the next undo is
    // the liquify, and the stale emitter's particles go with it.
    undo(&mut app);
    app.update();
    assert_eq!(emitters(&mut app), 0);
    assert_eq!(app.world().resource::<ParticleFluid>().particles.len(), 0);
    let after = bodies(&mut app);
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, before[0].0, "the same id");
    assert_eq!(after[0].1, before[0].1, "the same shape");
    assert!(
        after[0]
            .2
            .translation
            .abs_diff_eq(before[0].2.translation, 1e-5)
            && after[0].2.rotation.abs_diff_eq(before[0].2.rotation, 1e-5),
        "the same pose: {:?} -> {:?}",
        before[0].2,
        after[0].2
    );
}

/// The water tool's operation fills its outline; drain removes the water in
/// one undo step, and undo brings it back.
#[test]
fn add_water_fills_and_drain_removes_it() {
    let mut app = paused_app();
    run(&mut app, "(add-water (list -1 0 1 0 1 0.5 -1 0.5))");
    assert_eq!(emitters(&mut app), 1);
    set_state(&mut app, GameState::Playing);
    step(&mut app, 2);
    let area = particle_area(&app);
    assert!((area - 1.0).abs() <= 0.05, "particle area {area} m2");
    set_state(&mut app, GameState::Paused);
    run(&mut app, "(drain)");
    app.update();
    assert_eq!(emitters(&mut app), 0, "drain removes the emitter");
    assert_eq!(
        app.world().resource::<ParticleFluid>().particles.len(),
        0,
        "and its particles"
    );
    undo(&mut app);
    app.update();
    assert_eq!(emitters(&mut app), 1, "undo brings the water back");
}
