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
use gradiance::input::selection::Selection;
use gradiance::input::tools::box_tool::BoxToolPlugin;
use gradiance::input::tools::circle_tool::CircleToolPlugin;
use gradiance::input::tools::connector::ConnectorToolPlugin;
use gradiance::input::tools::drag_tool::DragToolPlugin;
use gradiance::input::tools::polygon_tool::PolygonToolPlugin;
use gradiance::input::tools::select_tool::SelectToolPlugin;
use gradiance::prelude::*;
use gradiance::ui::grid::GridSettings;
use rstest::{fixture, rstest};

// --- Setup Fixtures (Copied/Adapted from tool_tests.rs) ---

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
    app.add_plugins(ConnectorToolPlugin);
    app.add_plugins(DragToolPlugin);

    app.init_state::<ToolState>();
    app.init_resource::<CursorWorldPos>();
    app.init_resource::<CommandStack>();
    app.init_resource::<GridSettings>();
    app.init_resource::<ZIndex>();
    app.init_resource::<Selection>();

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
    app.update();
}

fn spawn_box(app: &mut App, pos: Vec2, z: f32) -> Entity {
    let entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            GravityScale(0.0), // Disable gravity to keep position deterministic
            Collider::cuboid(1.0, 1.0),
            Transform::from_xyz(pos.x, pos.y, z),
            GlobalTransform::default(),
            EditableBox {
                width: 2.0,
                height: 2.0,
            },
        ))
        .id();

    // Run updates to sync physics
    for _ in 0..5 {
        app.update();
    }
    entity
}

// --- Tests ---

// (1) Single-click tools (Select Tool behavior)

// 1a. Shape foreground, nothing behind
#[rstest]
fn test_single_click_foreground(mut app: App) {
    let e = spawn_box(&mut app, Vec2::new(10.0, 10.0), 0.0);
    set_tool(&mut app, ToolState::Select);

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&e));
    assert_eq!(selection.0.len(), 1);
}

// 1b. Shape foreground, another shape behind
#[rstest]
fn test_single_click_foreground_occluded(mut app: App) {
    // Spawn background shape (Z=0)
    let back = spawn_box(&mut app, Vec2::new(10.0, 10.0), 0.0);
    // Spawn foreground shape (Z=1)
    let front = spawn_box(&mut app, Vec2::new(10.0, 10.0), 1.0);

    set_tool(&mut app, ToolState::Select);

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&front), "Front entity (Higher Z) should be selected");
    assert!(!selection.0.contains(&back), "Back entity should not be selected");
}

// 1c. No shape clicked
#[rstest]
fn test_single_click_no_shape(mut app: App) {
    let e = spawn_box(&mut app, Vec2::new(10.0, 10.0), 0.0);

    // Select it first to verify clearing
    {
        let mut sel = app.world_mut().resource_mut::<Selection>();
        sel.add(e);
    }

    set_tool(&mut app, ToolState::Select);

    // Click empty space
    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.0.is_empty(), "Selection should be cleared");
}

// (2) Click-drag-release tools

// 2a. Click shape, release same shape (Select Tool: treated as click/small drag)
#[rstest]
fn test_drag_release_same_shape_small_move(mut app: App) {
    // Disable grid snap for small move test
    app.world_mut().resource_mut::<GridSettings>().snap = false;

    let pos = Vec2::new(10.0, 10.0);
    let e = spawn_box(&mut app, pos, 0.0);
    set_tool(&mut app, ToolState::Select);

    set_cursor(&mut app, pos);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    // Small move
    set_cursor(&mut app, pos + Vec2::new(0.1, 0.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update();

    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&e));

    // Check position (should have moved slightly)
    let t = app.world().get::<Transform>(e).unwrap();
    assert!((t.translation.x - (pos.x + 0.1)).abs() < 1e-5);
}

// 2b. Click shape, release background (Connector Tool: Pin to world)
#[rstest]
fn test_connector_drag_release_background(mut app: App) {
    let pos_a = Vec2::new(10.0, 10.0);
    let e_a = spawn_box(&mut app, pos_a, 0.0);
    let target_pos = Vec2::new(20.0, 20.0);

    set_tool(&mut app, ToolState::SpringJoint);

    // Start on shape A
    set_cursor(&mut app, pos_a);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    // Drag to background
    set_cursor(&mut app, target_pos);
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify joint creation
    let joint = app.world().get::<ImpulseJoint>(e_a).expect("Joint should be on entity A");
    let pin_entity = joint.parent; // Parent of joint is the Target Body (Pin)

    // Verify Pin Location
    let pin_transform = app.world().get::<Transform>(pin_entity).expect("Pin should have transform");
    assert!((pin_transform.translation.x - target_pos.x).abs() < 1e-5, "Pin X wrong");
    assert!((pin_transform.translation.y - target_pos.y).abs() < 1e-5, "Pin Y wrong");

    // Verify Local Anchor on A (Effector Position)
    // We clicked exactly at pos_a (center of box A).
    // Local anchor should be (0,0).
    let generic = match &joint.data {
        bevy_rapier2d::dynamics::TypedJoint::GenericJoint(g) => g,
        _ => panic!("Expected GenericJoint"),
    };
    let local_anchor_a = generic.raw.local_anchor1();
    assert!(local_anchor_a.coords.magnitude() < 1e-5, "Local anchor A should be zero (center) but is {:?}", local_anchor_a.coords);
}

// 2c. Click shape, release on new shape (Connector Tool: Joint between bodies)
#[rstest]
fn test_connector_drag_release_new_shape(mut app: App) {
    let pos_a = Vec2::new(10.0, 10.0);
    let pos_b = Vec2::new(20.0, 20.0);
    let e_a = spawn_box(&mut app, pos_a, 0.0);
    let e_b = spawn_box(&mut app, pos_b, 0.0);

    set_tool(&mut app, ToolState::SpringJoint);

    set_cursor(&mut app, pos_a);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, pos_b);
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify joint on A connected to B
    let joint = app.world().get::<ImpulseJoint>(e_a).expect("Joint should be on A");
    assert_eq!(joint.parent, e_b, "Joint should target entity B");

    // Verify Local Anchors (Effector Positions)
    // Clicked at centers of both boxes. Local anchors should be zero.
    let generic = match &joint.data {
        bevy_rapier2d::dynamics::TypedJoint::GenericJoint(g) => g,
        _ => panic!("Expected GenericJoint"),
    };
    let local_anchor_a = generic.raw.local_anchor1();
    let local_anchor_b = generic.raw.local_anchor2();
    assert!(local_anchor_a.coords.magnitude() < 1e-5, "Local anchor A should be zero");
    assert!(local_anchor_b.coords.magnitude() < 1e-5, "Local anchor B should be zero");
}

// 2d. Click background, release on shape (Connector Tool: Pin attached to shape)
#[rstest]
fn test_connector_drag_background_release_shape(mut app: App) {
    let start_pos = Vec2::new(0.0, 0.0);
    let shape_pos = Vec2::new(10.0, 10.0);
    let e_shape = spawn_box(&mut app, shape_pos, 0.0);

    set_tool(&mut app, ToolState::SpringJoint);

    set_cursor(&mut app, start_pos); // Background
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, shape_pos); // Shape
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Logic: A=None, B=Shape -> Swap -> A=Shape, B=None.
    // Anchor A should be at shape_pos (where we released).
    // Anchor B (Pin) should be at start_pos (where we started).

    // Verify joint on Shape
    let joint = app.world().get::<ImpulseJoint>(e_shape).expect("Joint should be on Shape");
    let pin_entity = joint.parent;

    // Verify Pin Location (should be at start_pos)
    let pin_transform = app.world().get::<Transform>(pin_entity).expect("Pin transform");
    assert!((pin_transform.translation.x - start_pos.x).abs() < 1e-5, "Pin should be at background start pos");
}

// 2e. Click background, release on background (Connector Tool: No-op)
#[rstest]
fn test_connector_drag_background_release_background(mut app: App) {
    set_tool(&mut app, ToolState::SpringJoint);

    set_cursor(&mut app, Vec2::new(0.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, Vec2::new(10.0, 10.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify no joints/entities (other than window/defaults)
    let mut joints = app.world_mut().query::<&ImpulseJoint>();
    assert_eq!(joints.iter(app.world()).count(), 0);
}
