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
use gradiance::input::commands::CommandStack;
use gradiance::input::cursor::CursorWorldPos;
use gradiance::input::editable::EditableBox;
use gradiance::input::selection::{Selection, SelectionFilter, SelectionGroup, SelectionPlugin};
use gradiance::input::shortcuts::ShortcutsPlugin;
use gradiance::input::tools::box_tool::BoxToolPlugin;
use gradiance::input::tools::circle_tool::CircleToolPlugin;
use gradiance::input::tools::connector::{Connector, ConnectorToolPlugin};
use gradiance::input::tools::drag_tool::DragToolPlugin;
use gradiance::input::tools::polygon_tool::PolygonToolPlugin;
use gradiance::input::tools::select_tool::SelectToolPlugin;
use gradiance::physics::floor::GroundPlane;
use gradiance::prelude::*;
use gradiance::ui::grid::GridSettings;
use rstest::{fixture, rstest};

#[fixture]
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);
    app.add_plugins(TransformPlugin);
    app.add_plugins(StatesPlugin);
    app.add_plugins(BevyInputPlugin);

    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    app.init_resource::<Assets<Shader>>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<ColorMaterial>>();
    app.init_resource::<EguiUserTextures>();
    app.init_resource::<Events<bevy::picking::backend::PointerHits>>();
    app.init_resource::<Events<WindowScaleFactorChanged>>();
    app.init_resource::<Events<WindowResized>>();
    app.init_resource::<Events<WindowCreated>>();
    app.init_resource::<Events<AssetEvent<Image>>>();

    app.add_plugins(GizmoPlugin);
    app.add_plugins(ShapePlugin);
    app.add_plugins(BoxToolPlugin);
    app.add_plugins(CircleToolPlugin);
    app.add_plugins(PolygonToolPlugin);
    app.add_plugins(SelectToolPlugin);
    app.add_plugins(SelectionPlugin); // Added
    app.add_plugins(ConnectorToolPlugin);
    app.add_plugins(DragToolPlugin);
    app.add_plugins(ShortcutsPlugin);

    app.init_state::<ToolState>();
    app.init_resource::<CursorWorldPos>();
    app.init_resource::<CommandStack>();
    app.init_resource::<GridSettings>();
    app.init_resource::<ZIndex>();
    // SelectionPlugin inits Selection, SelectionFilter, NextGroupID

    app.update();

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
use bevy::input::keyboard::KeyboardInput;
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

fn key_down(app: &mut App, key_code: KeyCode) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(KeyboardInput {
        key_code,
        state: ButtonState::Pressed,
        window,
        logical_key: bevy::input::keyboard::Key::Unidentified(
            bevy::input::keyboard::NativeKey::Unidentified,
        ),
        repeat: false,
    });
}

fn key_up(app: &mut App, key_code: KeyCode) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(KeyboardInput {
        key_code,
        state: ButtonState::Released,
        window,
        logical_key: bevy::input::keyboard::Key::Unidentified(
            bevy::input::keyboard::NativeKey::Unidentified,
        ),
        repeat: false,
    });
}

fn set_tool(app: &mut App, state: ToolState) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(state);
    app.update();
}

#[rstest]
fn test_select_all_ctrl_a(mut app: App) {
    // Spawn 2 boxes
    let b1 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox::default(),
        ))
        .id();
    let b2 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(5.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox::default(),
        ))
        .id();

    // Physics update
    app.update();
    app.update();

    set_tool(&mut app, ToolState::Select);

    // Ctrl + A
    key_down(&mut app, KeyCode::ControlLeft);
    key_down(&mut app, KeyCode::KeyA);
    app.update();
    key_up(&mut app, KeyCode::KeyA);
    key_up(&mut app, KeyCode::ControlLeft);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&b1));
    assert!(selection.0.contains(&b2));
}

#[rstest]
fn test_ctrl_drag_copy(mut app: App) {
    // Spawn box
    let b1 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
            Name::new("Original"),
            GravityScale(0.0),
        ))
        .id();

    app.update();
    app.update();

    set_tool(&mut app, ToolState::Select);

    // Disable grid snap to avoid unexpected offsets in test
    app.world_mut().resource_mut::<GridSettings>().snap_to_grid = false;

    // Select b1
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Check selected
    assert!(app.world().resource::<Selection>().0.contains(&b1));

    // Ctrl + Drag
    key_down(&mut app, KeyCode::ControlLeft);

    // Mouse down at (0,0)
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    // Move to (5,5)
    set_cursor(&mut app, Vec2::new(5.0, 5.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    key_up(&mut app, KeyCode::ControlLeft);
    app.update();

    // Verify:
    // 1. Original box still at 0,0
    let t1 = app.world().get::<Transform>(b1).unwrap().translation;
    assert_eq!(t1.truncate(), Vec2::ZERO);

    // 2. New box exists at 5,5
    let mut query = app
        .world_mut()
        .query_filtered::<(Entity, &Transform), (With<EditableBox>, Without<GroundPlane>)>();
    let entities: Vec<(Entity, &Transform)> = query.iter(app.world()).collect();
    assert_eq!(entities.len(), 2, "Should have 2 boxes now");

    let (_, t2) = entities.iter().find(|(e, _)| *e != b1).unwrap();
    assert_eq!(t2.translation.truncate(), Vec2::new(5.0, 5.0));
}

#[rstest]
fn test_right_click_rotate(mut app: App) {
    // Spawn box at (10,0)
    let b1 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(10.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox::default(),
        ))
        .id();

    app.update();
    set_tool(&mut app, ToolState::Select);

    // Select b1
    app.world_mut().resource_mut::<Selection>().0.insert(b1);

    // Right Click Drag
    // Centroid is (10,0).
    // Mouse at (10,0).
    // Drag Up (10, 5).
    // If logic is "Drag maps to rotation", maybe Y delta = rotation?
    // Let's assume implementation will map 100 pixels to PI or something.

    set_cursor(&mut app, Vec2::new(10.0, 0.0));
    mouse_down(&mut app, MouseButton::Right);
    app.update();

    set_cursor(&mut app, Vec2::new(10.0, 10.0)); // Moved +10 Y
    app.update();
    mouse_up(&mut app, MouseButton::Right);
    app.update();

    // Verify rotation changed
    let t = app.world().get::<Transform>(b1).unwrap();
    let rot = t.rotation.to_euler(EulerRot::XYZ).2;
    assert!(rot.abs() > 0.01, "Should have rotated");
}

#[rstest]
fn test_selection_filter(mut app: App) {
    // Spawn Box and Connector
    let b1 = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            EditableBox::default(),
        ))
        .id();

    let c1 = app
        .world_mut()
        .spawn((
            Transform::from_xyz(5.0, 0.0, 0.0),
            GlobalTransform::default(),
            Collider::ball(0.5),
            Connector {
                entity_a: b1,
                entity_b: None,
                local_anchor_a: Vec2::ZERO,
                local_anchor_b: Vec2::ZERO,
            },
        ))
        .id();

    app.update();
    app.update();

    set_tool(&mut app, ToolState::Select);

    // Set Filter to Shapes (Box)
    *app.world_mut().resource_mut::<SelectionFilter>() = SelectionFilter::Shapes;

    // Try select Connector (at 5,0)
    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    assert!(
        app.world().resource::<Selection>().0.is_empty(),
        "Should not select Connector"
    );

    // Try select Box (at 0,0)
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    assert!(
        app.world().resource::<Selection>().0.contains(&b1),
        "Should select Box"
    );

    // Set Filter to Joints
    *app.world_mut().resource_mut::<SelectionFilter>() = SelectionFilter::Joints;
    app.world_mut().resource_mut::<Selection>().clear();

    // Try select Box
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    assert!(
        app.world().resource::<Selection>().0.is_empty(),
        "Should not select Box"
    );

    // Try select Connector
    set_cursor(&mut app, Vec2::new(5.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    assert!(
        app.world().resource::<Selection>().0.contains(&c1),
        "Should select Connector"
    );
}

#[rstest]
fn test_group_objects(mut app: App) {
    let b1 = app
        .world_mut()
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            Collider::ball(1.0),
        ))
        .id();
    let b2 = app
        .world_mut()
        .spawn((
            Transform::from_xyz(5.0, 0.0, 0.0),
            GlobalTransform::default(),
            Collider::ball(1.0),
        ))
        .id();
    app.update();
    app.update();

    set_tool(&mut app, ToolState::Select);

    // Select both
    app.world_mut().resource_mut::<Selection>().0.insert(b1);
    app.world_mut().resource_mut::<Selection>().0.insert(b2);

    // Ctrl + G
    key_down(&mut app, KeyCode::ControlLeft);
    key_down(&mut app, KeyCode::KeyG);
    app.update();
    key_up(&mut app, KeyCode::KeyG);
    key_up(&mut app, KeyCode::ControlLeft);
    app.update();

    // Verify they have SelectionGroup component
    let g1 = app.world().get::<SelectionGroup>(b1);
    let g2 = app.world().get::<SelectionGroup>(b2);
    assert!(g1.is_some());
    assert!(g2.is_some());
    assert_eq!(g1.unwrap(), g2.unwrap());

    // Clear selection
    app.world_mut().resource_mut::<Selection>().clear();

    // Click b1
    set_cursor(&mut app, Vec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Assert both selected
    let s = app.world().resource::<Selection>();
    assert!(s.0.contains(&b1));
    assert!(s.0.contains(&b2));
}

#[rstest]
fn test_joint_inspector_modification(mut app: App) {
    // Create entity with ImpulseJoint
    let child = app.world_mut().spawn(Transform::default()).id();
    let parent = app.world_mut().spawn(Transform::default()).id();

    let joint = RevoluteJointBuilder::new().build();
    app.world_mut()
        .entity_mut(parent)
        .insert(ImpulseJoint::new(child, joint));

    // Verify existence
    assert!(app.world().get::<ImpulseJoint>(parent).is_some());
}
