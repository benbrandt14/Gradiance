use bevy::asset::AssetEvent;
use bevy::gizmos::GizmoPlugin;
use bevy::input::ButtonState;
use bevy::input::InputPlugin as BevyInputPlugin;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseButtonInput;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::window::{PrimaryWindow, WindowCreated, WindowResized, WindowScaleFactorChanged};

use bevy_egui::EguiUserTextures;
use bevy_prototype_lyon::plugin::ShapePlugin;
use gradiance::events::GameEventsPlugin;
use gradiance::geometry::extrusion::ExtrusionPlugin;
use gradiance::input::ToolState;
use gradiance::input::ZIndex;
use gradiance::input::commands::CommandStack;
use gradiance::input::cursor::CursorWorldPos;
use gradiance::input::event_handlers;
use gradiance::input::selection::{NextGroupID, Selection, SelectionFilter};
use gradiance::input::tools::box_tool::BoxToolPlugin;
use gradiance::input::tools::circle_tool::CircleToolPlugin;
use gradiance::input::tools::connector::ConnectorToolPlugin;
use gradiance::input::tools::drag_tool::DragToolPlugin;
use gradiance::input::tools::ground_tool::GroundToolPlugin;
use gradiance::input::tools::polygon_tool::PolygonToolPlugin;
use gradiance::input::tools::select_tool::SelectToolPlugin;
use gradiance::prelude::*;
use gradiance::ui::grid::GridSettings; // Add ExtrusionPlugin

#[allow(dead_code)]
pub fn create_test_app() -> App {
    let mut app = App::new();

    // Core plugins
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);
    app.add_plugins(TransformPlugin);
    app.add_plugins(StatesPlugin);
    app.add_plugins(BevyInputPlugin);
    app.add_plugins(WindowPlugin::default()); // Added WindowPlugin

    // Physics
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0));

    // Manual resource initialization for headless plugins
    app.init_resource::<Assets<Shader>>();
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<Image>>();
    app.init_resource::<Assets<ColorMaterial>>();
    app.init_resource::<Assets<StandardMaterial>>(); // Initialize StandardMaterial for Extrusion

    app.add_plugins(bevy_egui::EguiPlugin); // Added EguiPlugin
    // Add EditableShapePlugin explicitly to handle EditableShape component logic
    app.add_plugins(gradiance::input::editable_shape::EditableShapePlugin);
    app.init_resource::<EguiUserTextures>();
    app.init_resource::<Events<bevy::picking::backend::PointerHits>>();

    // Manually init Window events
    app.init_resource::<Events<WindowScaleFactorChanged>>();
    app.init_resource::<Events<WindowResized>>();
    app.init_resource::<Events<WindowCreated>>();
    app.init_resource::<Events<AssetEvent<Image>>>();
    app.init_resource::<Events<bevy::window::CursorMoved>>(); // Added missing events
    app.init_resource::<Events<bevy::input::keyboard::KeyboardInput>>();
    app.init_resource::<Events<bevy::input::mouse::MouseButtonInput>>();
    app.init_resource::<Events<bevy::input::mouse::MouseWheel>>();
    app.init_resource::<Events<bevy::window::WindowFocused>>();
    app.init_resource::<Events<bevy::window::Ime>>();

    // Plugins that rely on Render/Assets/Window but can run headless if resources exist
    app.add_plugins(GizmoPlugin);
    app.add_plugins(ShapePlugin);
    app.add_plugins(ExtrusionPlugin); // Add ExtrusionPlugin

    // Events
    app.add_plugins(GameEventsPlugin);

    // Register Event Handlers (Critical for tool tests)
    app.add_systems(
        Update,
        (
            event_handlers::handle_spawn_shape_event,
            event_handlers::handle_spawn_ground_event,
            event_handlers::handle_spawn_joint_event,
            event_handlers::handle_spawn_prismatic_joint_event,
            event_handlers::handle_spawn_fixed_joint_event,
            event_handlers::handle_undo_event,
            event_handlers::handle_redo_event,
            event_handlers::handle_duplicate_entities_event,
            event_handlers::handle_drag_entities_event,
            event_handlers::handle_commit_drag_event,
        ),
    );

    // Tools
    app.add_plugins(BoxToolPlugin);
    app.add_plugins(CircleToolPlugin);
    app.add_plugins(PolygonToolPlugin);
    app.add_plugins(SelectToolPlugin);
    app.add_plugins(ConnectorToolPlugin);
    app.add_plugins(DragToolPlugin);
    app.add_plugins(GroundToolPlugin);

    // Initial State
    app.init_state::<ToolState>();

    // Resources
    app.init_resource::<CursorWorldPos>();
    app.init_resource::<CommandStack>();
    app.init_resource::<GridSettings>();
    app.init_resource::<ZIndex>();
    app.init_resource::<Selection>();
    app.init_resource::<SelectionFilter>();
    app.init_resource::<NextGroupID>();
    app.init_resource::<gradiance::input::PointerOverUi>();

    // Shortcuts
    app.add_plugins(gradiance::input::shortcuts::ShortcutsPlugin);
    // Selection systems (drawing/grouping)
    app.add_plugins(gradiance::input::selection::SelectionPlugin);

    // Initial update
    app.update();

    app.update();

    app
}

#[allow(dead_code)]
pub fn set_cursor(app: &mut App, pos: Vec2) {
    let mut cursor = app.world_mut().resource_mut::<CursorWorldPos>();
    cursor.0 = Some(pos);
}

#[allow(dead_code)]
pub fn mouse_down(app: &mut App, button: MouseButton) {
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

#[allow(dead_code)]
pub fn mouse_up(app: &mut App, button: MouseButton) {
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

#[allow(dead_code)]
pub fn press_key(app: &mut App, key_code: KeyCode) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(KeyboardInput {
        key_code,
        logical_key: bevy::input::keyboard::Key::Unidentified(
            bevy::input::keyboard::NativeKey::Unidentified,
        ),
        state: ButtonState::Pressed,
        window,
        repeat: false,
    });
}

#[allow(dead_code)]
pub fn release_key(app: &mut App, key_code: KeyCode) {
    let window = app
        .world_mut()
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .single(app.world());
    app.world_mut().send_event(KeyboardInput {
        key_code,
        logical_key: bevy::input::keyboard::Key::Unidentified(
            bevy::input::keyboard::NativeKey::Unidentified,
        ),
        state: ButtonState::Released,
        window,
        repeat: false,
    });
}

#[allow(dead_code)]
pub fn set_tool(app: &mut App, state: ToolState) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(state);
    app.update(); // Process state change
}
