//! Particle-sim integration tests (`--features sim`): the shape→particles
//! authoring seam, end to end through the headless stack.

use crate::harness::{box_record, entity_of, headless_app, paused_app, step};
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::sim::bridge::Particles;
use gradiance::sim::groups::{GroupAttrs, ParticleGroups};
use gradiance::ui::context_menu::ParticleFillQueue;

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
