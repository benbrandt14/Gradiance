use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::math::Isometry;
use gradiance::geometry::extrusion::{ExtrudableShape, ExtrusionPlugin};
use gradiance::input::editable_shape::{EditableShape, ShapeType};
use gradiance::prelude::*;

#[test]
fn test_editable_shape_update_triggers_mesh_update() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(TransformPlugin);
    app.add_plugins(HierarchyPlugin);

    // Required resources
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();

    // Plugins
    app.add_plugins(ExtrusionPlugin);
    app.add_plugins(gradiance::input::editable_shape::EditableShapePlugin);

    // 1. Spawn Box (10x10)
    let shape = ShapeType::Box {
        width: 10.0,
        height: 10.0,
    };
    let entity = app
        .world_mut()
        .spawn((
            EditableShape {
                shape: shape.clone(),
            },
            ExtrudableShape,
            CollisionGroups::default(), // Default layers trigger mesh gen
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    // Run updates to process hooks and systems
    app.update();
    app.update();

    // Verify initial state
    let entity_ref = app.world().entity(entity);
    assert!(entity_ref.contains::<Path>(), "Path should be generated");
    assert!(
        entity_ref.contains::<Collider>(),
        "Collider should be generated"
    );
    assert!(
        entity_ref.contains::<Mesh3d>(),
        "Mesh3d should be generated"
    );

    let initial_mesh_handle = entity_ref.get::<Mesh3d>().unwrap().0.clone();

    // 2. Modify Shape (to Box 2x2)
    let new_shape = ShapeType::Box {
        width: 2.0,
        height: 2.0,
    };
    {
        let mut entity_mut = app.world_mut().entity_mut(entity);
        let mut editable = entity_mut.get_mut::<EditableShape>().unwrap();
        editable.shape = new_shape;
    }

    // Run updates
    app.update();
    app.update();

    // Verify updates
    let entity_ref = app.world().entity(entity);
    let final_mesh_handle = entity_ref.get::<Mesh3d>().unwrap().0.clone();

    assert_ne!(
        initial_mesh_handle, final_mesh_handle,
        "Mesh handle should change when shape changes"
    );

    // Check collider bounds approx
    let collider = entity_ref.get::<Collider>().unwrap();
    let aabb = collider.raw.compute_aabb(&Isometry::identity());
    let width = aabb.maxs.x - aabb.mins.x;
    assert!(
        (width - 2.0).abs() < 0.1,
        "Collider width should be approx 2.0, got {}",
        width
    );
}
