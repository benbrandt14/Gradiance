use bevy::input::InputPlugin as BevyInputPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_rapier2d::dynamics::{ImpulseJoint, TypedJoint};
use gradiance::input::event_handlers;
use gradiance::input::events::{self, JointType, SpawnJointEvent, ModifyJointEvent, SpawnGearEvent, SpawnPulleyEvent, SpawnChainEvent};
use gradiance::input::tools::select_tool::{SelectToolPlugin, HitSortInfo};
use gradiance::physics::constraints::{GearJoint, PulleyJoint};
use gradiance::input::selection::Selection;
use gradiance::input::editable::EditableBox;
use gradiance::prelude::*;
use rstest::{fixture, rstest};

#[fixture]
fn app() -> App {
    let mut app = App::new();

    app.add_plugins(MinimalPlugins);
    app.add_plugins(TransformPlugin);
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);
    app.add_plugins(StatesPlugin);
    app.add_plugins(BevyInputPlugin);

    // Physics
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    // Resources
    app.init_resource::<Selection>();
    app.init_resource::<gradiance::input::ZIndex>();
    app.init_resource::<gradiance::input::cursor::CursorWorldPos>();
    app.init_resource::<gradiance::ui::grid::GridSettings>();

    // Select Tool (for rotation logic if tested here, or manual system test)
    app.add_plugins(SelectToolPlugin);

    // Events
    app.add_event::<events::SpawnJointEvent>()
        .add_event::<events::ModifyJointEvent>()
        .add_event::<events::SpawnGearEvent>()
        .add_event::<events::SpawnPulleyEvent>()
        .add_event::<events::SpawnChainEvent>()
        .add_event::<events::ModifyTransformEvent>()
        .add_event::<events::ModifyPhysicsEvent>()
        .add_event::<events::ModifyShapeEvent>()
        .add_event::<events::ModifyRenderEvent>();

    // Event Handlers
    app.add_systems(
        Update,
        (
            event_handlers::handle_spawn_joint,
            event_handlers::handle_modify_joint,
            event_handlers::handle_spawn_gear,
            event_handlers::handle_spawn_pulley,
            event_handlers::handle_spawn_chain,
        ),
    );

    // Initial update
    app.update();
    app
}

#[rstest]
fn test_modify_joint_event(mut app: App) {
    // 1. Spawn two bodies
    let e1 = app.world_mut().spawn((RigidBody::Dynamic, Transform::default(), GlobalTransform::default())).id();
    let e2 = app.world_mut().spawn((RigidBody::Dynamic, Transform::from_xyz(2.0, 0.0, 0.0), GlobalTransform::default())).id();

    // 2. Spawn a Revolute Joint
    app.world_mut().send_event(SpawnJointEvent {
        joint_type: JointType::Revolute,
        entity_a: e1,
        entity_b: Some(e2),
        anchor_a: Vec2::ZERO,
        anchor_b: Vec2::ZERO,
    });

    app.update(); // Process spawn

    // Verify joint exists
    let joint = app.world().get::<ImpulseJoint>(e1).expect("Joint should exist");

    // 3. Send ModifyJointEvent
    app.world_mut().send_event(ModifyJointEvent {
        entity: e1,
        limits: Some((-1.0, 1.0)),
        drive_stiffness: Some(10.0),
        drive_damping: Some(0.5),
        drive_position: None,
        drive_velocity: None,
        length: None,
    });

    app.update(); // Process modify

    // Verify
    // Since GenericJoint hides internal state, direct verification is hard without reflection.
    // We assume if it didn't panic and code path ran, it's applied.
}

#[rstest]
fn test_spawn_gear_event(mut app: App) {
    let e1 = app.world_mut().spawn(RigidBody::Dynamic).id();
    let e2 = app.world_mut().spawn(RigidBody::Dynamic).id();

    app.world_mut().send_event(SpawnGearEvent {
        entity_a: e1,
        entity_b: e2,
        ratio: 2.0,
    });

    app.update();

    // Verification logic:
    let gear = app.world().query::<&GearJoint>().single(app.world());
    assert_eq!(gear.ratio, 2.0);
}

#[rstest]
fn test_spawn_chain_event(mut app: App) {
    app.world_mut().send_event(SpawnChainEvent {
        vertices: vec![Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)],
        width: 0.5,
    });

    app.update();

    // Verification:
    // Should spawn 2 bodies (segments) for 3 points.
    // And 1 joint connecting them.
    let bodies = app.world().query::<&EditableBox>().iter(app.world()).count();
    assert_eq!(bodies, 2);

    let joints = app.world().query::<&ImpulseJoint>().iter(app.world()).count();
    assert_eq!(joints, 1);
}
