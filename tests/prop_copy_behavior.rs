use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::geometry::extrusion::ExtrudableShape;
use gradiance::input::editable_shape::{EditableShape, ShapeType, generate_shape_components};

#[test]
fn test_copy_logic_extrusion_behavior() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(TransformPlugin);
    app.add_plugins(HierarchyPlugin);
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();
    app.add_plugins(gradiance::geometry::extrusion::ExtrusionPlugin);
    app.add_plugins(gradiance::input::editable_shape::EditableShapePlugin);

    let shape = ShapeType::Box {
        width: 1.0,
        height: 1.0,
    };

    // 1. Simulate "Actual Buggy" Duplicate (Missing ExtrudableShape)
    // The bug was that ExtrudableShape was not copied.
    let bad_entity = app
        .world_mut()
        .spawn((
            EditableShape {
                shape: shape.clone(),
            },
            // NO ExtrudableShape
            CollisionGroups::default(),
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    app.update(); // Run systems (adds Path via EditableShapePlugin)

    // Path is added by EditableShapePlugin
    assert!(app.world().get::<Path>(bad_entity).is_some());
    // Mesh3d is MISSING (because ExtrudableShape is missing)
    assert!(
        app.world().get::<Mesh3d>(bad_entity).is_none(),
        "Buggy duplicate should have no mesh"
    );

    // 2. Simulate "Fixed" Duplicate (With ExtrudableShape)
    // My fix adds ExtrudableShape (and manually adds Path/Collider for safety).
    let (path, collider) = generate_shape_components(&shape).unwrap();
    let good_entity = app
        .world_mut()
        .spawn((
            EditableShape {
                shape: shape.clone(),
            },
            ExtrudableShape, // This is the key fix
            path,
            collider,
            CollisionGroups::default(),
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    app.update();

    // Assert: Mesh3d IS present
    assert!(
        app.world().get::<Mesh3d>(good_entity).is_some(),
        "Fixed duplicate SHOULD have mesh"
    );
    let mesh_handle_1 = app.world().get::<Mesh3d>(good_entity).unwrap().0.clone();

    // 3. Test Property: Changing Layer updates Mesh
    app.world_mut()
        .entity_mut(good_entity)
        .insert(CollisionGroups::new(Group::GROUP_2, Group::GROUP_2));
    app.update();

    let mesh_handle_2 = app.world().get::<Mesh3d>(good_entity).unwrap().0.clone();
    assert_ne!(
        mesh_handle_1, mesh_handle_2,
        "Mesh handle should change when collision group changes"
    );
}
