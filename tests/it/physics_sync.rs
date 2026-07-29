//! Physics seam tests: authored components drive engine components, and
//! the simulation behaves qualitatively (falls, rests, pauses).

use crate::harness::{body_count, box_record, entity_of, headless_app, paused_app, step, undo};
use avian2d::prelude::{CollisionLayers, LockedAxes, RigidBody, Sensor};
use bevy::prelude::*;
use gradiance::prelude::*;

#[test]
fn spawned_body_gains_engine_components() {
    let mut app = headless_app();
    let mut record = box_record(Vec2::ZERO, 0.4, 0.2);
    record.depth = DepthBand {
        near: 0.1,
        far: 0.3, // layers 1..=2
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
        Some(&CollisionLayers::from_bits(0b0110, 0b0110)),
        "band bits are both memberships and filters (depth = collision)"
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
    let falling = box_record(Vec2::new(0.0, 2.0), 0.2, 0.2);
    let falling_id = falling.id;
    let mut floor = box_record(Vec2::new(0.0, -1.0), 10.0, 0.2);
    floor.physics.kind = BodyKind::Static;
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
    // Floor top is at -0.9, box half-height is 0.1 → rest around y = -0.8.
    assert!(y < 1.0, "box fell (y = {y})");
    assert!(
        (-0.95..=-0.60).contains(&y),
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
    // Floor top is at y = -0.9; a contact lives near that interface.
    assert!(
        contacts.iter().any(|c| (c.point.y - (-0.9)).abs() < 0.3),
        "a contact sits near the box-floor interface: {contacts:?}"
    );
}

#[derive(Resource, Default)]
struct PickProbe {
    point: Vec2,
    hits: Vec<Entity>,
}

fn probe_pick(physics: gradiance::physics::queries::PhysicsQueries, mut probe: ResMut<PickProbe>) {
    let point = probe.point;
    probe.hits = physics.bodies_at_point(point);
}

#[test]
fn picking_follows_a_body_moved_while_paused() {
    // Regression: while paused, avian's spatial BVH broad phase is frozen, so a
    // body a tool moves (by writing Transform directly) must still be pickable
    // at its new spot — the "objects go non-selectable until I play/pause" bug.
    // The read facade now hit-tests the live Transform, not the frozen BVH.
    let mut app = paused_app();
    app.init_resource::<PickProbe>();
    app.add_systems(Update, probe_pick);

    let record = box_record(Vec2::ZERO, 40.0, 40.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    step(&mut app, 2); // collider syncs while paused

    let entity = entity_of(&app, id).unwrap();
    // Move it far away by writing Transform directly, exactly as a paused
    // drag/move does — no physics step, so the BVH would stay at the old slot.
    app.world_mut()
        .get_mut::<Transform>(entity)
        .unwrap()
        .translation = Vec3::new(500.0, 0.0, 0.0);

    app.world_mut().resource_mut::<PickProbe>().point = Vec2::new(500.0, 0.0);
    step(&mut app, 2);
    assert!(
        app.world().resource::<PickProbe>().hits.contains(&entity),
        "a body moved while paused is pickable at its new position"
    );

    app.world_mut().resource_mut::<PickProbe>().point = Vec2::ZERO;
    step(&mut app, 2);
    assert!(
        !app.world().resource::<PickProbe>().hits.contains(&entity),
        "and not at its old position"
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

#[test]
fn attraction_clusters_and_repulsion_scatters() {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    let spawn_field_circle = |app: &mut App, x: f32, strength: f32| {
        let mut record = box_record(Vec2::new(x, 0.0), 30.0, 30.0);
        record.shape = ShapeDef::Circle { radius: 15.0 };
        record.field = Some(FieldSource {
            strength,
            falloff: FieldFalloff::Quadratic,
        });
        let id = record.id;
        app.world_mut().write_message(SpawnBodyIntent { record });
        app.update();
        id
    };
    let gap = |app: &mut App, a: StableId, b: StableId| {
        let pa = app
            .world()
            .get::<Transform>(entity_of(app, a).unwrap())
            .unwrap()
            .translation
            .truncate();
        let pb = app
            .world()
            .get::<Transform>(entity_of(app, b).unwrap())
            .unwrap()
            .translation
            .truncate();
        pa.distance(pb)
    };

    // "Select a bunch of circles and have them cluster": negative strength
    // (attraction, Algodoo convention) pulls them together.
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();
    let a = spawn_field_circle(&mut app, -80.0, -2000.0);
    let b = spawn_field_circle(&mut app, 80.0, -2000.0);
    let before = gap(&mut app, a, b);
    step(&mut app, 60);
    let after = gap(&mut app, a, b);
    assert!(
        after < before - 5.0,
        "attraction clusters the circles ({before} -> {after})"
    );

    // Positive strength repels.
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();
    let a = spawn_field_circle(&mut app, -40.0, 2000.0);
    let b = spawn_field_circle(&mut app, 40.0, 2000.0);
    let before = gap(&mut app, a, b);
    step(&mut app, 60);
    let after = gap(&mut app, a, b);
    assert!(
        after > before + 5.0,
        "repulsion scatters the circles ({before} -> {after})"
    );
}

#[test]
fn a_field_acts_on_plain_bodies_too() {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    // Algodoo attraction affects every body, not only other field sources.
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();

    let mut attractor = box_record(Vec2::ZERO, 40.0, 40.0);
    attractor.physics.kind = BodyKind::Static;
    attractor.field = Some(FieldSource {
        strength: -2000.0,
        falloff: FieldFalloff::Quadratic,
    });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: attractor });
    let plain = box_record(Vec2::new(200.0, 0.0), 20.0, 20.0);
    let plain_id = plain.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: plain });
    app.update();

    step(&mut app, 60);
    let x = app
        .world()
        .get::<Transform>(entity_of(&app, plain_id).unwrap())
        .unwrap()
        .translation
        .x;
    assert!(x < 190.0, "the plain body fell toward the attractor ({x})");
}

#[test]
fn set_in_orbit_produces_a_limit_cycle() {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    use gradiance::physics::fields::SetOrbitRequest;
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();

    let mut sun = box_record(Vec2::ZERO, 60.0, 60.0);
    sun.shape = ShapeDef::Circle { radius: 30.0 };
    sun.physics.kind = BodyKind::Static;
    sun.field = Some(FieldSource {
        strength: -4000.0,
        falloff: FieldFalloff::Quadratic,
    });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: sun });
    let mut moon = box_record(Vec2::new(220.0, 0.0), 16.0, 16.0);
    moon.shape = ShapeDef::Circle { radius: 8.0 };
    let moon_id = moon.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: moon });
    app.update();
    // One more frame so `sync_field_mass`'s deferred insert lands before
    // the orbit request samples the field.
    app.update();

    app.world_mut().write_message(SetOrbitRequest {
        targets: vec![moon_id],
    });
    app.update();

    // The orbit is a limit cycle: over several revolutions the radius stays
    // in a band instead of crashing in or escaping.
    let mut min_r = f32::MAX;
    let mut max_r = 0.0f32;
    for _ in 0..12 {
        step(&mut app, 30);
        let r = app
            .world()
            .get::<Transform>(entity_of(&app, moon_id).unwrap())
            .unwrap()
            .translation
            .truncate()
            .length();
        min_r = min_r.min(r);
        max_r = max_r.max(r);
    }
    assert!(
        min_r > 120.0 && max_r < 400.0,
        "orbit stayed in a band (r in [{min_r}, {max_r}])"
    );
}

#[test]
fn world_pin_prismatic_locks_rotation_by_default() {
    // A prismatic pinned to the background is a *guide*: the pinned body is
    // rotation-locked by default, so even a badly off-axis pin (gravity
    // torquing a long plank from its far end) never twists it.
    let mut app = headless_app();
    let mut plank = box_record(Vec2::new(0.0, 0.0), 200.0, 12.0);
    plank.pose.pos = Vec2::new(0.0, 100.0);
    let plank_id = plank.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: plank });
    app.update();
    app.world_mut().write_message(SpawnJointIntent {
        record: JointRecord {
            id: StableId::new(),
            def: JointDef {
                kind: JointKind::Slider {
                    axis: Vec2::Y,
                    limits: Some([-80.0, 80.0]),
                    motor: None,
                },
                common: JointCommon::default(),
                body_a: plank_id,
                // World pin at the plank's LEFT END: gravity torques it.
                body_b: None,
                anchor_a: Vec2::new(-90.0, 0.0),
                anchor_b: Vec2::new(-90.0, 100.0),
                rest_rot_a: 0.0,
                rest_rot_b: 0.0,
            },
        },
    });
    app.update();

    let plank_entity = entity_of(&app, plank_id).unwrap();
    assert!(
        app.world()
            .get::<LockedAxes>(plank_entity)
            .is_some_and(avian2d::prelude::LockedAxes::is_rotation_locked),
        "spawning a world-pin prismatic rotation-locks the pinned body"
    );

    step(&mut app, 300);
    let rot = PosRot::from_transform(
        app.world()
            .get::<Transform>(entity_of(&app, plank_id).unwrap())
            .unwrap(),
    )
    .rot;
    assert!(
        rot.abs() < 1e-3,
        "the guided plank never twists (rot after 5s = {rot})"
    );

    // Undoing the joint spawn removes the lock it added. The 5s run above is
    // its own undo step, so it takes the first press.
    undo(&mut app);
    undo(&mut app);
    let plank_entity = entity_of(&app, plank_id).unwrap();
    assert!(
        app.world().get::<LockedAxes>(plank_entity).is_none(),
        "undo removes the default rotation lock"
    );
}

#[test]
fn field_forces_are_equal_and_opposite() {
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    // One-sided setup on purpose: only the big circle carries a field, and
    // the bodies have different masses. Newton's third law says the system's
    // momentum still stays zero — the attractor is pulled toward what it
    // attracts.
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<gradiance::domain::settings::SimSettings>()
        .gravity = Vec2::ZERO;
    app.update();

    let mut attractor = box_record(Vec2::new(-60.0, 0.0), 60.0, 60.0);
    attractor.shape = ShapeDef::Circle { radius: 30.0 };
    attractor.field = Some(FieldSource {
        strength: -20_000.0,
        falloff: FieldFalloff::Quadratic,
    });
    let attractor_id = attractor.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: attractor });
    let plain = box_record(Vec2::new(120.0, 0.0), 24.0, 24.0);
    let plain_id = plain.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: plain });
    app.update();

    // Sample while the bodies are still in flight (before they collide and
    // contact forces complicate the picture).
    step(&mut app, 8);

    let momentum = |app: &App, id: StableId| -> Vec2 {
        let entity = entity_of(app, id).unwrap();
        let mass = app
            .world()
            .get::<avian2d::prelude::ComputedMass>(entity)
            .unwrap()
            .value();
        app.world()
            .get::<avian2d::prelude::LinearVelocity>(entity)
            .unwrap()
            .0
            * mass
    };
    let pa = momentum(&app, attractor_id);
    let pb = momentum(&app, plain_id);
    assert!(
        pb.length() > 1.0,
        "the field actually moved the plain body (|p| = {})",
        pb.length()
    );
    assert!(
        (pa + pb).length() < 0.05 * pb.length(),
        "momentum conserves: {pa:?} + {pb:?} ≈ 0"
    );
}

#[test]
fn cutting_a_field_source_conserves_its_field() {
    use bevy::ecs::system::SystemState;
    use gradiance::domain::field::{FieldFalloff, FieldSource};
    use gradiance::physics::fields::Fields;
    // The field couples through the source's mass (area × density), so
    // slicing an attractor redistributes its pull across the pieces instead
    // of doubling it: a distant probe sees (almost) the same acceleration.
    let mut app = paused_app();
    let mut source = box_record(Vec2::ZERO, 80.0, 40.0);
    source.field = Some(FieldSource {
        strength: -10_000.0,
        falloff: FieldFalloff::Quadratic,
    });
    app.world_mut()
        .write_message(SpawnBodyIntent { record: source });
    step(&mut app, 2); // let sync_field_mass derive the coupling mass

    let probe = Vec2::new(400.0, 0.0);
    let sample = |app: &mut App| -> Vec2 {
        let mut state: SystemState<Fields> = SystemState::new(app.world_mut());
        state
            .get(app.world())
            .expect("Fields param is always valid")
            .accel_at(probe, None)
            .value()
    };
    let before = sample(&mut app);
    assert!(before.length() > 0.1, "the probe feels the field");

    app.world_mut().write_message(CutIntent {
        a: Vec2::new(0.0, -40.0),
        b: Vec2::new(0.0, 40.0),
        width: 2.0,
    });
    step(&mut app, 2); // pieces spawn + their FieldMass derives

    let after = sample(&mut app);
    // The coupling mass is conserved exactly; the residual comes from the
    // falloff being measured over *surface* distance (the far piece's near
    // face moves to the cut plane). "Roughly the same" is the contract.
    assert!(
        (after - before).length() < 0.15 * before.length(),
        "cutting the source leaves the far field intact ({before:?} -> {after:?})"
    );
}

#[test]
fn tracers_sample_fading_trails_and_toggle_undoably() {
    use gradiance::domain::tracer::Tracer;
    use gradiance::render::tracer::TraceTrail;
    let mut app = headless_app();
    let mut record = box_record(Vec2::new(0.0, 300.0), 20.0, 20.0);
    record.tracer = Some(Tracer {
        fade_secs: 0.5,
        ..Default::default()
    });
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    // Falling under gravity: the trail accumulates samples.
    step(&mut app, 30);
    let entity = entity_of(&app, id).unwrap();
    let len = app.world().get::<TraceTrail>(entity).unwrap().0.len();
    assert!(len >= 5, "a moving traced body accumulates samples ({len})");

    // Samples older than the fade window expire (the trail is a window,
    // not an unbounded history).
    step(&mut app, 90);
    let now = app
        .world()
        .resource::<Time<avian2d::prelude::Physics>>()
        .elapsed_secs();
    let trail = app.world().get::<TraceTrail>(entity).unwrap();
    let (oldest, _) = trail.0.front().copied().unwrap();
    assert!(
        now - oldest < 0.6,
        "old samples expired (oldest is {}s old)",
        now - oldest
    );

    // Turning the tracer off is one undoable property edit; the derived
    // trail dies with the marker and undo restores the marker (a fresh
    // trail regrows -- the samples themselves are never authored).
    let tracer = *app.world().get::<Tracer>(entity).unwrap();
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![PropertyChange {
            id,
            old: PropertyValue::Tracer(Some(tracer)),
            new: PropertyValue::Tracer(None),
        }],
    });
    step(&mut app, 2);
    let entity = entity_of(&app, id).unwrap();
    assert!(app.world().get::<Tracer>(entity).is_none());
    assert!(
        app.world().get::<TraceTrail>(entity).is_none(),
        "the derived trail is removed with its marker"
    );

    // The sim is running here, so the first press rewinds the run (one undo
    // step, exactly as pause-then-undo would); the second reverts the edit.
    undo(&mut app);
    undo(&mut app);
    let entity = entity_of(&app, id).unwrap();
    assert_eq!(app.world().get::<Tracer>(entity), Some(&tracer));
}

#[test]
fn box_size_density_and_restitution_edits_apply_and_undo() {
    use avian2d::prelude::{ColliderDensity, Restitution};
    use gradiance::domain::shape::ShapeDef;
    let mut app = headless_app();
    let record = box_record(Vec2::ZERO, 20.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    let entity = entity_of(&app, id).unwrap();

    let old_shape = app.world().get::<ShapeDef>(entity).unwrap().clone();
    let authored = *app.world().get::<BodyPhysics>(entity).unwrap();
    let (old_density, old_restitution) = (authored.density, authored.restitution);

    // One batched edit of all three (the inspector commits each separately,
    // but the command path is identical) — proves the fields are editable.
    app.world_mut().write_message(PropertyEditIntent {
        changes: vec![
            PropertyChange {
                id,
                old: PropertyValue::Shape(old_shape.clone()),
                new: PropertyValue::Shape(ShapeDef::Box {
                    width: 60.0,
                    height: 40.0,
                }),
            },
            PropertyChange {
                id,
                old: PropertyValue::Density(old_density),
                new: PropertyValue::Density(Density(2.5)),
            },
            PropertyChange {
                id,
                old: PropertyValue::Restitution(old_restitution),
                new: PropertyValue::Restitution(0.9),
            },
        ],
    });
    app.update();

    let entity = entity_of(&app, id).unwrap();
    assert_eq!(
        app.world().get::<ShapeDef>(entity).unwrap(),
        &ShapeDef::Box {
            width: 60.0,
            height: 40.0
        },
        "box resize applied"
    );
    assert!((app.world().get::<ColliderDensity>(entity).unwrap().0 - 2.5).abs() < 1e-6);
    assert!(
        (app.world().get::<Restitution>(entity).unwrap().coefficient - 0.9).abs() < 1e-6,
        "restitution applied"
    );

    // Running sim: first press rewinds the run, second reverts the edit.
    undo(&mut app);
    undo(&mut app);
    let entity = entity_of(&app, id).unwrap();
    assert_eq!(app.world().get::<ShapeDef>(entity).unwrap(), &old_shape);
    assert!(
        (app.world()
            .get::<BodyPhysics>(entity)
            .unwrap()
            .density
            .value()
            - old_density.value())
        .abs()
            < 1e-6
    );
}

#[derive(Resource)]
struct ProbeTarget(StableId);

#[derive(Resource, Default)]
struct CapturedImpulse(Vec2);

fn capture_impulse(
    target: Option<Res<ProbeTarget>>,
    index: Res<gradiance::core::ids::IdIndex>,
    physics: gradiance::physics::queries::PhysicsQueries,
    mut out: ResMut<CapturedImpulse>,
) {
    if let Some(entity) = target.and_then(|t| index.entity(t.0)) {
        out.0 = physics.net_contact_impulse(entity).value();
    }
}

#[test]
fn net_contact_impulse_reads_a_resting_bodys_weight() {
    // The probe/plot read: a box resting on the floor feels an upward
    // normal impulse of about m*g*dt per step.
    let mut app = headless_app();
    app.init_resource::<CapturedImpulse>();
    app.add_systems(Update, capture_impulse);
    let (falling_id, _floor) = falling_box_scene(&mut app);
    app.insert_resource(ProbeTarget(falling_id));

    step(&mut app, 240); // land and settle

    let impulse = app.world().resource::<CapturedImpulse>().0;
    assert!(impulse.y > 0.0, "support pushes up ({impulse:?})");
    let dt = app
        .world()
        .resource::<Time<Fixed>>()
        .timestep()
        .as_secs_f32();
    let force = impulse.y / dt;
    // Weight = area x density x |g| = 0.2*0.2*1 * 10.
    let weight = 0.04 * 10.0;
    assert!(
        (force - weight).abs() < 0.3 * weight,
        "contact force approximates the weight ({force} vs {weight})"
    );
}
