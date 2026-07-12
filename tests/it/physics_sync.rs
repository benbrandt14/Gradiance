//! Physics seam tests: authored components drive engine components, and
//! the simulation behaves qualitatively (falls, rests, pauses).

use crate::harness::{body_count, box_record, entity_of, headless_app, paused_app, step};
use avian2d::prelude::{CollisionLayers, LockedAxes, RigidBody, Sensor};
use bevy::prelude::*;
use gradiance::prelude::*;

#[test]
fn spawned_body_gains_engine_components() {
    let mut app = headless_app();
    let mut record = box_record(Vec2::ZERO, 40.0, 20.0);
    record.layers = LayerMask32 {
        memberships: 0b0110,
        filters: 0xFF,
    };
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 2);

    let entity = entity_of(&app, id).unwrap();
    let world = app.world();
    assert_eq!(world.get::<RigidBody>(entity), Some(&RigidBody::Dynamic));
    assert!(
        world.get::<avian2d::prelude::Collider>(entity).is_some(),
        "collider derived from ShapeDef"
    );
    assert_eq!(
        world.get::<CollisionLayers>(entity),
        Some(&CollisionLayers::from_bits(0b0110, 0xFF))
    );
}

#[test]
fn polygon_shapes_get_decomposed_colliders() {
    let mut app = headless_app();
    let mut record = box_record(Vec2::ZERO, 1.0, 1.0);
    // A concave L-shape.
    record.shape = ShapeDef::Polygon {
        outline: vec![
            Vec2::new(-20.0, -20.0),
            Vec2::new(20.0, -20.0),
            Vec2::new(20.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 20.0),
            Vec2::new(-20.0, 20.0),
        ],
        holes: vec![],
    };
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 2);

    let entity = entity_of(&app, id).unwrap();
    assert!(
        app.world()
            .get::<avian2d::prelude::Collider>(entity)
            .is_some()
    );
    assert_eq!(body_count(&mut app), 1);
}

#[test]
fn sensor_and_rotation_lock_follow_props() {
    let mut app = headless_app();
    let record = box_record(Vec2::ZERO, 10.0, 10.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 2);
    let entity = entity_of(&app, id).unwrap();
    assert!(app.world().get::<Sensor>(entity).is_none());
    assert!(app.world().get::<LockedAxes>(entity).is_none());

    // Simulate a property command's authored mutation.
    {
        let mut e = app.world_mut().entity_mut(entity);
        e.insert(RigidBody::Kinematic);
        e.insert(Sensor);
        e.insert(LockedAxes::ROTATION_LOCKED);
    }
    step(&mut app, 2);
    assert!(app.world().get::<Sensor>(entity).is_some());
    assert!(app.world().get::<LockedAxes>(entity).is_some());
    assert_eq!(
        app.world().get::<RigidBody>(entity),
        Some(&RigidBody::Kinematic)
    );

    {
        let mut e = app.world_mut().entity_mut(entity);
        e.remove::<Sensor>();
        e.remove::<LockedAxes>();
    }
    step(&mut app, 2);
    assert!(app.world().get::<Sensor>(entity).is_none());
    assert!(app.world().get::<LockedAxes>(entity).is_none());
}

/// Spawns a dynamic box above a static floor and returns `(box id, floor id)`.
fn falling_box_scene(app: &mut App) -> (StableId, StableId) {
    let falling = box_record(Vec2::new(0.0, 200.0), 20.0, 20.0);
    let falling_id = falling.id;
    let mut floor = box_record(Vec2::new(0.0, -100.0), 1000.0, 20.0);
    floor.physics.rigid_body = RigidBody::Static;
    let floor_id = floor.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: falling });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: floor });
    step(app, 2);
    (falling_id, floor_id)
}

#[test]
fn dynamic_bodies_fall_and_rest_on_static_ground() {
    let mut app = headless_app();
    let (falling_id, _floor) = falling_box_scene(&mut app);

    step(&mut app, 240); // 4 simulated seconds

    let entity = entity_of(&app, falling_id).unwrap();
    let y = app.world().get::<Transform>(entity).unwrap().translation.y;
    // Floor top is at -90, box half-height is 10 → rest around y = -80.
    assert!(y < 100.0, "box fell (y = {y})");
    assert!(
        (-95.0..=-60.0).contains(&y),
        "box rests on the floor (y = {y})"
    );
}

#[derive(Resource, Default)]
struct CapturedContacts(Vec<gradiance::physics::queries::ContactSample>);

fn capture_contacts(
    physics: gradiance::physics::queries::PhysicsQueries,
    mut out: ResMut<CapturedContacts>,
) {
    out.0 = physics.contact_points();
}

#[test]
fn resting_bodies_report_contacts_through_the_facade() {
    // The read facade the contact overlay (and future plotters/scripts) use: a
    // box resting on the floor produces touching contacts near the interface.
    let mut app = headless_app();
    app.init_resource::<CapturedContacts>();
    app.add_systems(Update, capture_contacts);
    let (_falling, _floor) = falling_box_scene(&mut app);

    step(&mut app, 240); // land and settle

    let contacts = &app.world().resource::<CapturedContacts>().0;
    assert!(
        !contacts.is_empty(),
        "a box resting on the floor generates contacts"
    );
    // Floor top is at y = -90; a contact lives near that interface.
    assert!(
        contacts.iter().any(|c| (c.point.y - (-90.0)).abs() < 30.0),
        "a contact sits near the box-floor interface: {contacts:?}"
    );
}

#[test]
fn pausing_freezes_the_simulation() {
    let mut app = headless_app();
    let (falling_id, _floor) = falling_box_scene(&mut app);

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    step(&mut app, 2); // apply transition

    let entity = entity_of(&app, falling_id).unwrap();
    let y_before = app.world().get::<Transform>(entity).unwrap().translation.y;
    step(&mut app, 60);
    let y_after = app.world().get::<Transform>(entity).unwrap().translation.y;
    assert!(
        (y_before - y_after).abs() < 0.5,
        "paused body must not move ({y_before} → {y_after})"
    );

    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    step(&mut app, 62);
    let y_resumed = app.world().get::<Transform>(entity).unwrap().translation.y;
    assert!(
        y_resumed < y_after - 1.0,
        "resumed body falls again ({y_after} → {y_resumed})"
    );
}

#[test]
fn timestep_setting_applies_to_the_fixed_clock() {
    let mut app = paused_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .timestep_hz = 120.0;
    app.update();

    let dt = app
        .world()
        .resource::<Time<Fixed>>()
        .timestep()
        .as_secs_f64();
    assert!((dt - 1.0 / 120.0).abs() < 1e-9, "fixed dt followed ({dt})");
}

#[test]
fn substep_trace_records_one_entry_per_substep() {
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::DebugSettings>()
        .show_substeps = true;
    let record = box_record(Vec2::new(0.0, 200.0), 40.0, 20.0);
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    step(&mut app, 3); // a few physics steps while falling

    let substeps = app.world().resource::<avian2d::prelude::SubstepCount>().0 as usize;
    let trace = app.world().resource::<gradiance::physics::SubstepTrace>();
    assert_eq!(
        trace.0.len(),
        substeps,
        "the trace holds exactly the last step's substeps"
    );
    assert!(
        trace.0.iter().all(|frame| frame.len() == 1),
        "every substep recorded the one dynamic body"
    );
}
