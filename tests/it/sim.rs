//! Particle-sim integration tests (`--features sim`): the shape→particles
//! authoring seam, end to end through the headless stack.

use crate::harness::{box_record, entity_of, headless_app, paused_app, step};
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::prelude::*;
use gradiance::sim::bridge::{Particles, SimConfig};
use gradiance::sim::groups::{GroupAttrs, ParticleGroups};
use gradiance::sim::interactions::SetParticleOrbitRequest;
use gradiance::ui::context_menu::ParticleFillQueue;

/// Spawns a circular attractor body (negative strength = attraction) at `x`.
fn attractor(app: &mut App, x: f32, strength: f32) -> StableId {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    let mut record = box_record(Vec2::new(x, 0.0), 30.0, 30.0);
    record.shape = ShapeDef::Circle { radius: 15.0 };
    record.physics = BodyPhysics::fixed();
    record.field = Some(FieldSource {
        strength,
        falloff: FieldFalloff::Quadratic,
    });
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(app, 2);
    id
}

/// Spawns a body and returns its id.
fn spawn_body(app: &mut App, record: BodyRecord) -> StableId {
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

#[test]
fn fill_shape_request_materializes_a_group_of_particles() {
    let mut app = paused_app();
    let record = box_record(Vec2::new(30.0, -10.0), 100.0, 60.0);
    let depth = DepthBand {
        near: 10.0,
        far: 25.0,
    };
    let id = {
        let mut r = record;
        r.depth = depth;
        spawn_body(&mut app, r)
    };
    assert!(entity_of(&app, id).is_some(), "body spawned");

    // The context-menu action pushes a fill request; the sim drains it.
    app.world_mut()
        .resource_mut::<ParticleFillQueue>()
        .requests
        .push((id, 400));
    app.update();

    let particles = app.world().resource::<Particles>();
    let n = particles.0.len();
    assert!(
        (150..=700).contains(&n),
        "≈400 particles fill the box, got {n}"
    );
    // Every particle lies inside the body's world-space box (±half + slop).
    assert!(
        particles
            .0
            .pos
            .iter()
            .all(|p| (p.x - 30.0).abs() <= 52.0 && (p.y + 10.0).abs() <= 32.0),
        "particles are inside the filled body"
    );
    // They belong to a freshly-allocated group that inherited the body's
    // depth band — so the cloud obeys collision layers as one entity.
    let group = particles.0.group[0];
    assert!(group >= 1, "a new group (not the default bucket) was made");
    let groups = app.world().resource::<ParticleGroups>();
    assert_eq!(groups.get(group).depth, depth, "group inherited the band");

    // The request queue was drained.
    assert!(
        app.world()
            .resource::<ParticleFillQueue>()
            .requests
            .is_empty()
    );
}

#[test]
fn fill_respects_the_particle_budget() {
    let mut app = paused_app();
    let id = spawn_body(&mut app, box_record(Vec2::ZERO, 200.0, 200.0));
    // Ask for far more than the budget; the fill clamps to it.
    let budget = app
        .world()
        .resource::<gradiance::sim::bridge::SimConfig>()
        .max_particles;
    app.world_mut()
        .resource_mut::<ParticleFillQueue>()
        .requests
        .push((id, budget * 4));
    app.update();
    assert!(app.world().resource::<Particles>().0.len() <= budget);
}

/// Spawns a static platform box and returns its id.
fn platform(app: &mut App, pos: Vec2, w: f32, h: f32) -> StableId {
    let mut record = box_record(pos, w, h);
    record.physics = BodyPhysics::fixed();
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(app, 2);
    id
}

/// Drops `n` particles from `y` above the origin into group `group`.
fn drop_particles(app: &mut App, n: usize, y: f32, group: u32) {
    let mut particles = app.world_mut().resource_mut::<Particles>();
    for i in 0..n {
        let x = (i as f32 - n as f32 / 2.0) * 2.0;
        particles.0.push(Vec2::new(x, y), Vec2::ZERO, 1.0, group);
    }
}

#[test]
fn particles_rest_on_a_body_of_the_same_layer() {
    let mut app = headless_app(); // Playing → gravity + collision run
    // A wide static platform; top face at y = -80.
    platform(&mut app, Vec2::new(0.0, -100.0), 400.0, 40.0);
    // Particles in the default group (band 0..10) overlap the platform's
    // default band, so they collide.
    drop_particles(&mut app, 60, 0.0, 0);
    step(&mut app, 240);

    let ys: Vec<f32> = app
        .world()
        .resource::<Particles>()
        .0
        .pos
        .iter()
        .map(|p| p.y)
        .collect();
    let min_y = ys.iter().copied().fold(f32::MAX, f32::min);
    assert!(
        min_y > -85.0,
        "particles rest on the platform (top ≈ -80), none sank through: min y = {min_y}"
    );
}

#[test]
fn a_particle_stream_nudges_a_light_dynamic_body() {
    let mut app = headless_app(); // Playing → gravity + collision + coupling
    // A light dynamic box hanging in space (default group/band 0..10;
    // `box_record` bodies are dynamic by default).
    let record = box_record(Vec2::new(0.0, 0.0), 40.0, 40.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 2);
    let body = entity_of(&app, id).expect("body spawned");

    // Turn off scene gravity on particles so the momentum exchange is the
    // only horizontal driver, and disable it on the body's fall by reading
    // only the x-drift (the reaction is a sideways stream from the left).
    // Fire a dense fast stream from the left, moving +x, at the body's height.
    {
        let mut particles = app.world_mut().resource_mut::<Particles>();
        for i in 0..200 {
            let y = (i % 20) as f32 - 10.0; // spread across the body's face
            particles
                .0
                .push(Vec2::new(-60.0, y), Vec2::new(400.0, 0.0), 1.0, 0);
        }
    }
    let x0 = app
        .world()
        .entity(body)
        .get::<Transform>()
        .map_or(0.0, |t| t.translation.x);
    step(&mut app, 30);
    let x1 = app
        .world()
        .entity(body)
        .get::<Transform>()
        .map_or(0.0, |t| t.translation.x);
    assert!(
        x1 > x0 + 0.5,
        "the leftward stream pushed the body to the right (Newton's third law): x {x0} → {x1}"
    );
}

#[test]
fn a_group_field_response_scales_how_it_feels_a_field() {
    let mut app = headless_app();
    // Isolate the field: no scene gravity on particles, no self-gravity.
    app.world_mut().resource_mut::<SimSettings>().gravity = Vec2::ZERO;
    app.world_mut().resource_mut::<SimConfig>().feel_gravity = false;
    // A strong attractor to the right of the origin.
    attractor(&mut app, 200.0, -4000.0);
    // Group 0 (default) responds fully; a second group ignores fields.
    let inert = app
        .world_mut()
        .resource_mut::<ParticleGroups>()
        .add(GroupAttrs {
            field_response: 0.0,
            ..GroupAttrs::default()
        });
    // One responsive particle (group 0) and one inert (group `inert`), both
    // at the origin, both at rest.
    {
        let mut particles = app.world_mut().resource_mut::<Particles>();
        particles.0.push(Vec2::ZERO, Vec2::ZERO, 1.0, 0);
        particles.0.push(Vec2::ZERO, Vec2::ZERO, 1.0, inert);
    }
    step(&mut app, 20);

    let particles = app.world().resource::<Particles>();
    // The responsive particle drifted toward the attractor (+x); the inert
    // one stayed put (field_response 0 → no field acceleration).
    let responsive = particles.0.pos[0];
    let ignored = particles.0.pos[1];
    assert!(
        responsive.x > 1.0,
        "field_response 1.0 pulls the particle toward the attractor: {responsive:?}"
    );
    assert!(
        ignored.length() < 1e-2,
        "field_response 0.0 leaves the particle inert: {ignored:?}"
    );
}

#[test]
fn particles_can_be_set_in_orbit_around_an_attractor() {
    let mut app = headless_app();
    app.world_mut().resource_mut::<SimSettings>().gravity = Vec2::ZERO;
    app.world_mut().resource_mut::<SimConfig>().feel_gravity = false;
    // Attractor at the origin; a particle sitting out on +x at rest.
    attractor(&mut app, 0.0, -4000.0);
    app.world_mut()
        .resource_mut::<Particles>()
        .0
        .push(Vec2::new(150.0, 0.0), Vec2::ZERO, 1.0, 0);
    // Set every particle in orbit (one-shot velocity assignment).
    app.world_mut()
        .write_message(SetParticleOrbitRequest { group: None });
    app.update();

    let v = app.world().resource::<Particles>().0.vel[0];
    // The orbit velocity is tangential (perpendicular to the radius, which is
    // along ±x here) and non-trivial.
    assert!(
        v.length() > 10.0,
        "set-in-orbit gives the particle orbital speed: {v:?}"
    );
    assert!(
        v.x.abs() < v.y.abs(),
        "orbit velocity is tangential (mostly ±y for a radius along x): {v:?}"
    );
}

#[test]
fn particles_on_a_disjoint_layer_pass_through() {
    let mut app = headless_app();
    platform(&mut app, Vec2::new(0.0, -100.0), 400.0, 40.0);
    // A group on a deep, non-overlapping band (layers ~10) — collision is
    // gated on depth overlap, so these fall straight through the platform.
    let group = app
        .world_mut()
        .resource_mut::<ParticleGroups>()
        .add(GroupAttrs {
            depth: DepthBand {
                near: 100.0,
                far: 110.0,
            },
            ..GroupAttrs::default()
        });
    drop_particles(&mut app, 40, 0.0, group);
    step(&mut app, 240);

    let min_y = app
        .world()
        .resource::<Particles>()
        .0
        .pos
        .iter()
        .map(|p| p.y)
        .fold(f32::MAX, f32::min);
    assert!(
        min_y < -120.0,
        "a disjoint-layer cloud passes through the platform: min y = {min_y}"
    );
}
