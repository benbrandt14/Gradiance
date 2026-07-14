//! Particle-sim integration tests (`--features sim`): the shape→particles
//! authoring seam, end to end through the headless stack.

use crate::harness::{box_record, entity_of, paused_app};
use bevy::prelude::*;
use gradiance::prelude::*;
use gradiance::sim::bridge::Particles;
use gradiance::sim::groups::ParticleGroups;
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
