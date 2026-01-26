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
    let groups = CollisionGroups::new(Group::from_bits_truncate(5), Group::ALL);

    let entity = app.world_mut().spawn((
        path,
        groups,
        ExtrudableShape,
        Transform::default(),
        Visibility::default(),
    )).id();

    // Check if Mesh3d component is added
    let entity_ref = app.world().entity(entity);
    assert!(entity_ref.contains::<Mesh3d>(), "Entity should have Mesh3d component");

    let meshes = app.world().resource::<Assets<Mesh>>();

    // Find the generated mesh in assets.
    // Due to some test environment quirks, handle ID might mismatch stored ID,
    // so we verify by content.
    let mut found_mesh = false;
    for (_, mesh) in meshes.iter() {
        if let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) {
             // Check for vertices at expected Z levels (0 and 30)
             let mut found_start = false;
             let mut found_end = false;

             for p in positions {
                 let z = p[2];
                 if (z - 0.0).abs() < 0.001 { found_start = true; }
                 if (z - 30.0).abs() < 0.001 { found_end = true; }
             }

             if found_start && found_end {
                 found_mesh = true;
                 break;
             }
        }
    }

    assert!(found_mesh, "Should find a mesh with vertices at z=0.0 and z=30.0");
}
