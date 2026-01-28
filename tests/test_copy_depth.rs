use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use gradiance::geometry::extrusion::{ExtrudableShape, ExtrusionPlugin};

#[test]
fn test_copy_reproduces_depth_bug() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AssetPlugin::default());
    app.add_plugins(ExtrusionPlugin);
    app.init_asset::<Mesh>();
    app.init_asset::<StandardMaterial>();

    // Create a path (rectangle)
    let path_builder = GeometryBuilder::build_as(&shapes::Rectangle {
        extents: Vec2::new(10.0, 10.0),
        origin: shapes::RectangleOrigin::Center,
        ..default()
    });

    // 1. Setup Source Entity with specific CollisionGroups (shallow)
    // Layer 0 only (bit 0). Min=0, Max=0. Depth = 10.0.
    let group = Group::GROUP_1; // Layer 0
    let source_entity = app.world_mut().spawn((
        CollisionGroups::new(group, group),
        ExtrudableShape,
        path_builder.clone(),
    )).id();

    app.update();

    // Verify source mesh depth
    {
        let source_mesh_handle = app.world().get::<Mesh3d>(source_entity).unwrap().0.clone();
        let meshes = app.world().resource::<Assets<Mesh>>();
        let source_mesh = meshes.get(&source_mesh_handle).unwrap();
        let source_depth = get_mesh_depth(source_mesh);
        assert!((source_depth - 10.0).abs() < 0.1, "Source depth should be 10.0, got {}", source_depth);
    }

    // 2. Simulate Copy Logic using a system
    app.add_systems(Update, move |mut commands: Commands, query: Query<(&CollisionGroups, &Path)>| {
        if let Ok((cg, path)) = query.get(source_entity) {
            let mut cmd = commands.spawn(Transform::default());
            // Simulate Bad Order: ExtrudableShape BEFORE CollisionGroups
            cmd.insert(ExtrudableShape);
            cmd.insert(path.clone());
            cmd.insert(*cg);
        }
    });

    app.update(); // Queue commands
    app.update(); // Execute commands

    // 3. Find the new entity
    let mut new_entity = None;
    for (e, _cg) in app.world_mut().query::<(Entity, &CollisionGroups)>().iter(app.world()) {
        if e != source_entity {
            new_entity = Some(e);
            break;
        }
    }

    assert!(new_entity.is_some(), "New entity should have been spawned");
    let new_entity = new_entity.unwrap();

    // Check mesh depth of the new entity
    let new_mesh_handle = app.world().get::<Mesh3d>(new_entity);
    assert!(new_mesh_handle.is_some(), "New entity should have a mesh");

    let new_mesh_handle = new_mesh_handle.unwrap().0.clone();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let new_mesh = meshes.get(&new_mesh_handle).unwrap();
    let new_depth = get_mesh_depth(new_mesh);

    // If bug exists, new_depth will be 320.0 (default) instead of 10.0
    assert!((new_depth - 10.0).abs() < 0.1, "New entity depth should be 10.0, but got {}", new_depth);
}

fn get_mesh_depth(mesh: &Mesh) -> f32 {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
    let mut min_z = f32::MAX;
    let mut max_z = f32::MIN;
    for p in positions {
        if p[2] < min_z { min_z = p[2]; }
        if p[2] > max_z { max_z = p[2]; }
    }
    max_z - min_z
}
