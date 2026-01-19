use bevy::gizmos::GizmoPlugin;
use bevy::input::InputPlugin as BevyInputPlugin;
use bevy::math::DVec2;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use bevy_prototype_lyon::plugin::ShapePlugin;
use gradiance::input::ToolState;
use gradiance::input::ZIndex;
use gradiance::input::commands::CommandStack;
use gradiance::input::cursor::CursorWorldPos;
use gradiance::input::editable::{EditableBox, EditableCircle};
use gradiance::input::selection::Selection;
use gradiance::input::tools::box_tool::BoxToolPlugin;
use gradiance::input::tools::circle_tool::CircleToolPlugin;
use gradiance::input::tools::connector::ConnectorToolPlugin;
use gradiance::input::tools::polygon_tool::PolygonToolPlugin;
use gradiance::input::tools::select_tool::SelectToolPlugin;
use gradiance::prelude::*;
use gradiance::ui::grid::GridSettings;
use rstest::{fixture, rstest};

#[fixture]
fn app() -> App {
    let mut app = App::new();

    // Core plugins
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    // app.add_plugins(bevy_hierarchy::HierarchyPlugin); // TODO: Fix unresolved import
    app.add_plugins(TransformPlugin);
    app.add_plugins(StatesPlugin);
    app.add_plugins(BevyInputPlugin);

    // Physics
    app.add_plugins(PhysicsPlugins::default());

    // Rendering / Shapes (headless but needed for components)
    app.add_plugins(GizmoPlugin);
    app.add_plugins(ShapePlugin);

    // Resources usually added by InputPlugin or UiPlugin
    app.init_resource::<ButtonInput<MouseButton>>();
    app.init_resource::<ButtonInput<KeyCode>>();
    app.init_resource::<CursorWorldPos>();
    app.init_resource::<CommandStack>();
    app.init_resource::<GridSettings>();
    app.init_resource::<ZIndex>();
    app.init_resource::<Selection>(); // For SelectTool

    // Tool Plugins
    app.add_plugins(BoxToolPlugin);
    app.add_plugins(CircleToolPlugin);
    app.add_plugins(PolygonToolPlugin);
    app.add_plugins(SelectToolPlugin);
    app.add_plugins(ConnectorToolPlugin);

    // Initial State
    app.init_state::<ToolState>();

    // Initial update to register everything
    app.update();

    app
}

fn set_cursor(app: &mut App, pos: DVec2) {
    let mut cursor = app.world_mut().resource_mut::<CursorWorldPos>();
    cursor.0 = Some(pos);
}

fn mouse_down(app: &mut App, button: MouseButton) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    input.press(button);
}

fn mouse_up(app: &mut App, button: MouseButton) {
    let mut input = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
    input.release(button);
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
    set_cursor(&mut app, DVec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, DVec2::new(10.0, 10.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

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
    set_cursor(&mut app, DVec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();

    set_cursor(&mut app, DVec2::new(5.0, 0.0));
    app.update();

    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

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
    set_cursor(&mut app, DVec2::ZERO);
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 2
    set_cursor(&mut app, DVec2::new(10.0, 0.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Point 3
    set_cursor(&mut app, DVec2::new(0.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Close loop (click near start)
    set_cursor(&mut app, DVec2::new(0.1, 0.1));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update(); // Process command

    // Verify
    let mut query = app
        .world_mut()
        .query_filtered::<Entity, (With<RigidBody>, With<Collider>)>();
    // Note: We expect 1 entity.
    // Depending on pre-existing entities (ground?), count might be higher?
    // The fixture does NOT spawn ground.
    assert_eq!(query.iter(app.world()).count(), 1);
}

#[rstest]
fn test_selection_tool(mut app: App) {
    // 1. Spawn a box manually
    let box_entity = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::rectangle(2.0, 2.0),
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
    set_cursor(&mut app, DVec2::new(10.0, 10.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify selection
    let selection = app.world().resource::<Selection>();
    assert!(selection.0.contains(&box_entity), "Box should be selected");

    // 3. Click elsewhere
    set_cursor(&mut app, DVec2::new(0.0, 0.0));
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
            Collider::rectangle(2.0, 2.0),
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
    set_cursor(&mut app, DVec2::new(5.0, 5.0));
    mouse_down(&mut app, MouseButton::Left);
    app.update();
    mouse_up(&mut app, MouseButton::Left);
    app.update();

    // Verify:
    // A RevoluteJoint component should be added to a new visual entity which is child of box_entity,
    // OR directly to box_entity?
    // Looking at connector.rs: SpawnJointCommand spawns a visual_entity, sets parent to entity_a.
    // Adds RevoluteJoint to visual_entity (if entity_b exists) OR pin_entity.
    // Wait, if pin_entity (None entity_b), it spawns a Static pin_entity.
    // Then adds RevoluteJoint to visual_entity connecting entity_a and pin_entity.

    // So we check if box_entity has a child with RevoluteJoint.
    let children = app
        .world()
        .get::<Children>(box_entity)
        .expect("Box should have children (visual)");
    let has_joint = children
        .iter()
        .any(|child| app.world().get::<RevoluteJoint>(child).is_some());

    assert!(
        has_joint,
        "Should have created a RevoluteJoint on a child entity"
    );
}
