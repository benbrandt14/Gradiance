//! Particle water (brief S-10 spike): 2 000 particles settle in a tank and
//! float a box of half their density.

#![allow(clippy::unwrap_used)]

use crate::harness::{paused_app, step};
use bevy::prelude::*;
use gradiance::physics::particles::ParticleFluid;
use gradiance::prelude::*;
use gradiance::script::bridge::ScriptInputs;
use std::time::Duration;

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

/// The water surface away from `x0` (outside `|x - x0| < keep_out`): the
/// mean of the highest particle in each 0.1 m column, plus half a spacing.
fn surface(fluid: &ParticleFluid, x0: f32, keep_out: f32) -> f32 {
    let mut tops = std::collections::BTreeMap::<i32, f32>::new();
    for p in &fluid.particles.pos {
        if (p.x - x0).abs() < keep_out {
            continue;
        }
        let column = (p.x / 0.1).floor() as i32;
        let top = tops.entry(column).or_insert(f32::MIN);
        *top = top.max(p.y);
    }
    let mean = tops.values().sum::<f32>() / tops.len() as f32;
    mean + 0.5 * fluid.params.spacing
}

/// The spike scene and the brief's behaviour test: 2 000 particles fall
/// into a 2 m tank and settle; a box of half the settled water's density
/// floats half submerged (+-10 %). The step time is printed for the
/// decision note.
#[test]
fn a_box_of_half_the_density_floats_half_submerged() {
    let mut app = paused_app();
    // Floor and two walls: a tank 2 m wide inside.
    run(
        &mut app,
        "(begin (spawn-box 0 -0.1 3 0.2) (spawn-box -1.1 1.5 0.2 3) (spawn-box 1.1 1.5 0.2 3))",
    );
    run(
        &mut app,
        "(begin (set-static 0 1) (set-static 1 1) (set-static 2 1))",
    );
    {
        let mut fluid = app.world_mut().resource_mut::<ParticleFluid>();
        let s = fluid.params.spacing;
        fluid
            .particles
            .fill_box(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 1.6), s);
        assert_eq!(fluid.particles.len(), 2000);
    }
    set_state(&mut app, GameState::Playing);
    // Four seconds to settle, timing the last second.
    step(&mut app, 180);
    let mut spent = Duration::ZERO;
    for _ in 0..60 {
        step(&mut app, 1);
        spent += app.world().resource::<ParticleFluid>().last_step;
    }
    eprintln!(
        "particle water: 2000 particles, {:.2} ms per physics step",
        spent.as_secs_f64() * 1000.0 / 60.0
    );
    let (density, level) = {
        let fluid = app.world().resource::<ParticleFluid>();
        let level = surface(fluid, 0.0, 0.0);
        let mass = fluid.particles.len() as f32 * fluid.params.particle_mass;
        (mass / (2.0 * level), level)
    };
    eprintln!("settled level {level:.3} m, density {density:.0} kg/m2");
    // Drop a 0.4 m box of half that density onto the water.
    set_state(&mut app, GameState::Paused);
    run(&mut app, &format!("(spawn-box 0 {} 0.4 0.4)", level + 0.25));
    run(&mut app, &format!("(set-density 3 {})", 0.5 * density));
    let floater = bodies(&mut app)[3];
    set_state(&mut app, GameState::Playing);
    // Six seconds to come to rest (measured: within 1 % after five).
    step(&mut app, 360);
    let mut fractions = Vec::new();
    for _ in 0..60 {
        step(&mut app, 1);
        let y = app.world().get::<Transform>(floater).unwrap().translation.y;
        let water = surface(app.world().resource::<ParticleFluid>(), 0.0, 0.35);
        fractions.push((water - (y - 0.2)) / 0.4);
    }
    let fraction = fractions.iter().sum::<f32>() / fractions.len() as f32;
    assert!(
        (fraction - 0.5).abs() <= 0.05,
        "half submerged (+-10 %): {fraction:.3}"
    );
}
