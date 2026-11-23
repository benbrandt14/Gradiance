//! Air buoyancy (brief S-22): with `SimSettings.air_buoyancy` on, every
//! dynamic body feels the weight of the air it displaces against gravity,
//! so a body less dense than the air rises. Off by default.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, headless_app, step};
use avian2d::prelude::{ColliderDensity, LinearVelocity};
use bevy::prelude::*;
use gradiance::domain::settings::SimSettings;
use gradiance::op::invoke;
use gradiance::prelude::*;
use serde_json::json;

fn spawn_body(app: &mut App, density: f32) -> StableId {
    let mut record = box_record(Vec2::new(0.0, 5.0), 0.5, 0.5);
    record.physics.density = ColliderDensity(density);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    id
}

fn velocity(app: &App, id: StableId) -> Vec2 {
    app.world()
        .get::<LinearVelocity>(entity_of(app, id).unwrap())
        .map_or(Vec2::ZERO, |v| v.0)
}

fn g(app: &App) -> f32 {
    app.world()
        .resource::<SimSettings>()
        .gravity
        .value()
        .length()
}

/// The brief's behaviour test: a body of density 0.6 in air of 1.2 rises
/// with net acceleration g × (1.2 / 0.6 − 1), within 3 %. No drag is set,
/// so the rise is the buoyancy alone.
#[test]
fn a_body_lighter_than_air_rises_at_the_archimedes_rate() {
    let mut app = headless_app();
    invoke(
        app.world_mut(),
        "sim.set",
        json!({"path": "air_buoyancy", "value": 1.0}),
    )
    .expect("the switch is a sim path");
    assert!(app.world().resource::<SimSettings>().air_buoyancy);
    let light = spawn_body(&mut app, 0.6);
    let heavy = spawn_body(&mut app, 2.4);
    let (v0, h0) = (velocity(&app, light), velocity(&app, heavy));
    step(&mut app, 60);
    let rate = (velocity(&app, light).y - v0.y) / 1.0;
    let expected = g(&app) * (1.2 / 0.6 - 1.0);
    assert!(
        (rate - expected).abs() < 0.03 * expected,
        "rise {rate} m/s^2 vs {expected}"
    );
    // A body twice as dense as the air falls at g × (1.2 / 2.4 − 1) = −g/2.
    let fall = (velocity(&app, heavy).y - h0.y) / 1.0;
    let expected = g(&app) * (1.2 / 2.4 - 1.0);
    assert!(
        (fall - expected).abs() < 0.03 * expected.abs(),
        "fall {fall} m/s^2 vs {expected}"
    );
}

/// Off (the default), the same light body falls at g.
#[test]
fn without_air_buoyancy_a_light_body_falls() {
    let mut app = headless_app();
    assert!(!SimSettings::default().air_buoyancy, "off by default");
    let light = spawn_body(&mut app, 0.6);
    let v0 = velocity(&app, light);
    step(&mut app, 60);
    let rate = velocity(&app, light).y - v0.y;
    assert!((rate + g(&app)).abs() < 0.03 * g(&app), "{rate}");
}
