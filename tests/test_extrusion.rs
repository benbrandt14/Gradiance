use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::geometry::extrusion::{ExtrudableShape, ExtrusionPlugin};

#[test]
fn test_extrusion_generation() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(TransformPlugin);
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);

    // Required resources
    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();

    // Extrusion Plugin
    app.add_plugins(ExtrusionPlugin);

    // Create a path (Rectangle)
    let path = GeometryBuilder::build_as(&shapes::Rectangle {
        extents: Vec2::new(10.0, 10.0),
        origin: shapes::RectangleOrigin::Center,
        radii: None,
    });

    // Spawn entity
    // Layers 0 (1) and 2 (4) -> bits 0101 -> 5
    // min_i = 0. max_i = 2.
    // Z direction is now NEGATIVE.
    // z_front = -(0 * 10) = 0.0.
    // depth = (2 - 0 + 1) * 10 = 30.0.
    // z_back = 0.0 - 30.0 = -30.0.
    let groups = CollisionGroups::new(Group::from_bits_truncate(5), Group::ALL);

    let entity = app
        .world_mut()
        .spawn((
            path,
            groups,
            ExtrudableShape,
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    // Run update to process commands queued by hooks
    app.update();

    // Check if Mesh3d component is added
    let entity_ref = app.world().entity(entity);
    assert!(
        entity_ref.contains::<Mesh3d>(),
        "Entity should have Mesh3d component"
    );

    let meshes = app.world().resource::<Assets<Mesh>>();

    // Find the generated mesh in assets.
    let mut found_mesh = false;
    for (_, mesh) in meshes.iter() {
        if check_mesh_depth(mesh, 0.0, -30.0) {
            found_mesh = true;
            break;
        }
    }

    assert!(
        found_mesh,
        "Should find a mesh with vertices at z=0.0 and z=-30.0"
    );
}

#[test]
fn test_extrusion_update_on_collision_change() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(TransformPlugin);
    app.add_plugins(bevy::hierarchy::HierarchyPlugin);

    app.init_resource::<Assets<Mesh>>();
    app.init_resource::<Assets<StandardMaterial>>();

    app.add_plugins(ExtrusionPlugin);

    let path = GeometryBuilder::build_as(&shapes::Rectangle {
        extents: Vec2::new(10.0, 10.0),
        origin: shapes::RectangleOrigin::Center,
        radii: None,
    });

    // Initial: Layer 0 only. Depth 10. z_front 0, z_back -10.
    let initial_groups = CollisionGroups::new(Group::GROUP_1, Group::ALL);

    let entity = app
        .world_mut()
        .spawn((
            path,
            initial_groups,
            ExtrudableShape,
            Transform::default(),
            Visibility::default(),
        ))
        .id();

    app.update();

    // Verify initial depth
    {
        let entity_ref = app.world().entity(entity);
        let mesh_handle = entity_ref.get::<Mesh3d>().expect("Mesh3d missing").0.clone();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mesh = meshes.get(&mesh_handle).expect("Mesh asset missing");

        assert!(check_mesh_depth(mesh, 0.0, -10.0), "Initial depth incorrect");
    }

    // Change CollisionGroups: Layer 1 only. min_i=1, max_i=1.
    // z_front = -10. depth = 10. z_back = -20.
    let new_groups = CollisionGroups::new(Group::GROUP_2, Group::ALL);

    app.world_mut().entity_mut(entity).insert(new_groups);

    app.update(); // Trigger system

    // Verify updated depth
    {
        let entity_ref = app.world().entity(entity);
        let mesh_handle = entity_ref.get::<Mesh3d>().expect("Mesh3d missing").0.clone();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mesh = meshes.get(&mesh_handle).expect("Mesh asset missing");

        assert!(check_mesh_depth(mesh, -10.0, -20.0), "Updated depth incorrect");
    }
}

fn check_mesh_depth(mesh: &Mesh, z_front: f32, z_back: f32) -> bool {
    if let Some(positions) = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .and_then(|a| a.as_float3())
    {
        let mut found_start = false;
        let mut found_end = false;

        for p in positions {
            let z = p[2];
            if (z - z_front).abs() < 0.001 {
                found_start = true;
            }
            if (z - z_back).abs() < 0.001 {
                found_end = true;
            }
        }
        return found_start && found_end;
    }
    false
}
