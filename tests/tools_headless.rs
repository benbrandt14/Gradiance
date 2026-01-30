use bevy::prelude::*;
use gradiance::events::{GameEventsPlugin, SpawnShapeEvent};
use gradiance::input::commands::CommandStack;
use gradiance::input::editable_shape::ShapeType;

#[test]
fn test_event_handler_spawn_shape() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    // We only need asset storage for Mesh/Material, not the actual rendering pipeline.
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.add_plugins(GameEventsPlugin);

    // We only need the event handler system and CommandStack
    app.init_resource::<CommandStack>();
    app.add_systems(Update, gradiance::input::event_handlers::handle_spawn_shape_event);

    // Mock ExtrusionPlugin hook?
    // The panic was "Requested resource Assets<Mesh> does not exist".
    // `app.init_asset::<Mesh>()` should fix that.

    // Send event
    app.world_mut().send_event(SpawnShapeEvent {
        position: Vec2::new(10.0, 10.0),
        shape: ShapeType::Box { width: 5.0, height: 5.0 },
    });

    // Run schedule
    // app.update();
    // Note: Extrusion logic might fail if it tries to access Render resources (e.g. if it uses MeshMaterial3d which might need rendering setup?)
    // Actually `MeshMaterial3d` is just a component with a Handle. It should be fine.

    // However, if `gradiance::geometry::extrusion` does more than just add components...
    // Let's try running it.
    app.update();

    // Verify CommandStack
    let stack = app.world().resource::<CommandStack>();
    assert_eq!(stack.history_len(), 1);
    assert_eq!(stack.current_index(), 1);
}
