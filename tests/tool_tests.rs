use bevy::asset::AssetEvent;
use bevy::gizmos::GizmoPlugin;
use bevy::input::InputPlugin as BevyInputPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::{PrimaryWindow, WindowCreated, WindowResized, WindowScaleFactorChanged};

use bevy_egui::EguiUserTextures;
use bevy_prototype_lyon::plugin::ShapePlugin;
use gradiance::input::ToolState;
use gradiance::input::ZIndex;
use gradiance::input::cursor::CursorWorldPos;
use gradiance::input::editable::{EditableBox, EditableCircle};
use gradiance::input::selection::Selection;
use gradiance::input::tools::box_tool::BoxToolPlugin;
use gradiance::input::tools::circle_tool::CircleToolPlugin;
use gradiance::input::tools::connector::ConnectorToolPlugin;
use gradiance::input::tools::drag_tool::DragToolPlugin;
use gradiance::input::tools::polygon_tool::PolygonToolPlugin;
use gradiance::input::tools::select_tool::SelectToolPlugin;
use gradiance::input::event_handlers;
use gradiance::input::events;
use gradiance::prelude::*;
use gradiance::ui::grid::GridSettings;
use rstest::{fixture, rstest};

#[fixture]
fn app() -> App {
    let mut app = App::new();

    // Core plugins
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);
    app.add_plugins(TransformPlugin);
    app.add_plugins(StatesPlugin);
    app.add_plugins(BevyInputPlugin);
    // WindowPlugin is NOT added to avoid Winit/Window creation issues in headless env.

    // Physics
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    // Manual resource initialization for headless plugins
    app.init_resource::<Assets<Shader>>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<ColorMaterial>>();

    app.init_resource::<EguiUserTextures>();
    app.init_resource::<Events<bevy::picking::backend::PointerHits>>();

    // Manually init Window events
    app.init_resource::<Events<WindowScaleFactorChanged>>();
    app.init_resource::<Events<WindowResized>>();
    app.init_resource::<Events<WindowCreated>>();
    app.init_resource::<Events<AssetEvent<Image>>>();

    // Plugins that rely on Render/Assets/Window but can run headless if resources exist
    app.add_plugins(GizmoPlugin);
    app.add_plugins(ShapePlugin);

    // Tools
    app.add_plugins(BoxToolPlugin);
    app.add_plugins(CircleToolPlugin);
    app.add_plugins(PolygonToolPlugin);
    app.add_plugins(SelectToolPlugin);
    app.add_plugins(ConnectorToolPlugin);
    app.add_plugins(DragToolPlugin);

    // Initial State
    app.init_state::<ToolState>();

    // Resources
    app.init_resource::<CursorWorldPos>();
    app.init_resource::<GridSettings>();
    app.init_resource::<ZIndex>();
    app.init_resource::<Selection>();

    // Events and Handlers
    app.add_event::<events::SpawnBoxEvent>()
        .add_event::<events::SpawnCircleEvent>()
        .add_event::<events::SpawnPolygonEvent>()
        .add_event::<events::SpawnGroundEvent>()
        .add_event::<events::SpawnJointEvent>()
        .add_event::<events::ModifyTransformEvent>()
        .add_event::<events::ModifyPhysicsEvent>()
        .add_event::<events::ModifyShapeEvent>()
        .add_event::<events::ModifyRenderEvent>();

    app.add_systems(
        Update,
        (
            event_handlers::handle_spawn_box,
            event_handlers::handle_spawn_circle,
            event_handlers::handle_spawn_polygon,
            event_handlers::handle_spawn_ground,
            event_handlers::handle_spawn_joint,
            event_handlers::handle_modify_transform,
            event_handlers::handle_modify_physics,
            event_handlers::handle_modify_shape,
            event_handlers::handle_modify_render,
        ),
    );


    // Initial update
    app.update();

    // Spawn Primary Window entity (headless) for systems that query it
    app.world_mut().spawn((
        Window {
            title: "Headless Test Window".into(),
            ..default()
        },
        PrimaryWindow,
    ));
    app.update();

    app
}

fn set_cursor(app: &mut App, pos: Vec2) {
    let mut cursor = app.world_mut().resource_mut::<CursorWorldPos>();
    cursor.0 = Some(pos);
}

use bevy::input::ButtonState;
use bevy::input::mouse::MouseButtonInput;

fn mouse_down(app: &mut App, button: MouseButton) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(MouseButtonInput {
        button,
        state: ButtonState::Pressed,
        window,
    });
}

fn mouse_up(app: &mut App, button: MouseButton) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(MouseButtonInput {
        button,
        state: ButtonState::Released,
        window,
    });
}

fn set_tool(app: &mut App, state: ToolState) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(state);
    app.update(); // Process state change
}

#[rstest]
fn test_box_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Box);

    // Drag from (0,0) to (10,10)
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Emit Event
    app.update(); // Process Event

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableBox>, With<RigidBody>)>();
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_circle_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Circle);

    // Drag from (0,0) to (5,0) -> Radius 5
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Emit Event
    app.update(); // Process Event

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<EditableCircle>, With<RigidBody>)>();
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_polygon_tool_spawn(mut app: App) {
    set_tool(&mut app, ToolState::Polygon);

    // Triangle: (0,0) -> (10,0) -> (0,10) -> (0,0)

    // Point 1
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 2
    set_cursor(&mut app, Vec2::new(10.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 3
    set_cursor(&mut app, Vec2::new(0.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Close loop (click near start)
    set_cursor(&mut app, Vec2::new(0.1, 0.1));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Emit Event
    app.update(); // Process Event

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<RigidBody>, With<Collider>)>();
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_selection_tool(mut app: App) {
    // 1. Spawn a box manually
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(10.0, 10.0, 0.0),
            GlobalTransform::default(), // Important for spatial query
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Run physics update to populate spatial index
    // Needs multiple updates to ensure transform propagation and broadphase update
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::Select);

    // 2. Click on the box
    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify selection
    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&box_entity), "Box should be selected");

    // 3. Click elsewhere
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify cleared
    let selection = app.world().resource::<Selection>();
    assert!(selection.0.is_empty(), "Selection should be cleared");
}

#[rstest]
fn test_joint_tool_pin(mut app: App) {
    // 1. Spawn a box
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(5.0, 5.0, 0.0),
            GlobalTransform::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Update physics
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::RevoluteJoint);

    // 2. Click on the box to pin it
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Emit
    app.update(); // Process

    // Verify:
    let has_joint = app.world().get::<ImpulseJoint>(box_entity).is_some();

    assert!(
        has_joint,
        "Should have created an ImpulseJoint on the entity"
    );
}

#[rstest]
fn test_drag_tool(mut app: App) {
    // 1. Spawn a dynamic box
    let _box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(5.0, 5.0, 0.0),
            GlobalTransform::default(),
            Velocity::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Update physics
    for _ in 0..5 {
        app.update();
    }

    set_tool(&mut app, ToolState::Drag);

    // 2. Click on the box
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update(); // Trigger spawn of hand entity

    // Verify hand entity exists
    let mut hand_query = app
        .world_mut()
        .query_filtered::<Entity, (With<RigidBody>, Without<Collider>)>();
    // There should be one hand entity (KinematicPositionBased)
    let hands: Vec<Entity> = hand_query.iter(app.world()).collect();
    // Filter to ensure it's not the box (Box has Collider)
    assert_eq!(hands.len(), 1, "Should have spawned one hand entity");

    // 3. Move cursor
    let target_pos = Vec2::new(10.0, 10.0);
    set_cursor(&mut app, target_pos);
    app.update(); // Hand should move

    // Verify hand moved
    let hand_entity = hands[0];
    let hand_transform = app.world().get::<Transform>(hand_entity).unwrap();
    assert_eq!(
        hand_transform.translation.truncate(),
        target_pos,
        "Hand should follow cursor"
    );

    // 4. Release mouse
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify hand is despawned
    assert!(
        app.world().get_entity(hand_entity).is_err(),
        "Hand should be despawned"
    );
}
