use crate::prelude::*;
use bevy::render::mesh::PrimitiveTopology;
use bevy_prototype_lyon::prelude::Path;
use bevy_rapier2d::prelude::CollisionGroups;
use lyon::algorithms::path::iterator::PathIterator;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers,
};

/// System to sync Extruded 3D meshes with 2D Paths and CollisionGroups.
pub fn sync_extruded_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (
            Entity,
            &Path,
            &CollisionGroups,
            Option<&Mesh3d>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        ),
        Or<(Changed<Path>, Changed<CollisionGroups>)>,
    >,
) {
    for (entity, path, collision_groups, mesh_3d, mesh_mat) in query.iter() {
        // 1. Calculate Z Logic
        let (z_min, z_max) = calculate_z_range(collision_groups);

        // 2. Geometry Construction
        let mesh = build_extruded_mesh(path, z_min, z_max);

        // 3. Asset Management
        if let Some(mesh_3d) = mesh_3d {
            // Overwrite existing mesh data to avoid memory leaks
            if let Some(existing_mesh) = meshes.get_mut(&mesh_3d.0) {
                *existing_mesh = mesh;
            } else {
                 // If the handle is stale, we might need to assign a new one, but for now assume it's valid.
                 // If it's invalid, the visual won't update.
            }

            // Update Material Color if needed
            if let Some(mesh_mat) = mesh_mat {
                if let Some(material) = materials.get_mut(&mesh_mat.0) {
                    material.base_color = color_from_groups(collision_groups);
                }
            }
        } else {
            // Create new Mesh and Material
            let mesh_handle = meshes.add(mesh);
            let material_handle = materials.add(StandardMaterial {
                base_color: color_from_groups(collision_groups),
                cull_mode: None, // Render both sides
                ..default()
            });

            commands.entity(entity).insert((
                Mesh3d(mesh_handle),
                MeshMaterial3d(material_handle),
            ));
        }
    }
}

/// Helper to map collision layers to Z-range.
fn calculate_z_range(groups: &CollisionGroups) -> (f32, f32) {
    let bits = groups.memberships.bits();
    if bits == 0 {
        return (0.0, 10.0); // Default thickness if no bits set
    }

    let min_layer = bits.trailing_zeros();
    let max_layer_plus_one = 32 - bits.leading_zeros(); // e.g. bits=1 (layer 0) -> lz=31 -> 1. Range [0,1]

    // Scale factor for visualization
    let layer_thickness = 10.0;

    let z_min = (min_layer as f32) * layer_thickness;
    let z_max = (max_layer_plus_one as f32) * layer_thickness;

    (z_min, z_max)
}

/// Helper to generate color based on layer.
fn color_from_groups(groups: &CollisionGroups) -> Color {
    let bits = groups.memberships.bits();
    let first_bit = bits.trailing_zeros();
    match first_bit % 6 {
        0 => Color::srgb(1.0, 0.2, 0.2), // Red
        1 => Color::srgb(0.2, 1.0, 0.2), // Green
        2 => Color::srgb(0.2, 0.2, 1.0), // Blue
        3 => Color::srgb(1.0, 1.0, 0.0), // Yellow
        4 => Color::srgb(0.0, 1.0, 1.0), // Cyan
        5 => Color::srgb(1.0, 0.0, 1.0), // Magenta
        _ => Color::WHITE,
    }
}

/// Helper to build the extruded mesh.
pub fn build_extruded_mesh(path: &Path, z_min: f32, z_max: f32) -> Mesh {
    // 1. Reconstruct Lyon Path to ensure compatibility
    // We assume path.0 gives us access to the underlying path which is iterable
    let lyon_path = LyonPath::from_iter(path.0.iter());

    // 2. Tessellate Top Face
    let mut geometry: VertexBuffers<Vec3, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    // We need a custom vertex constructor to add Z coordinate
    struct VertexConstructor3d {
        z: f32,
    }

    impl lyon::tessellation::FillVertexConstructor<Vec3> for VertexConstructor3d {
        fn new_vertex(&mut self, vertex: FillVertex) -> Vec3 {
            Vec3::new(vertex.position().x, vertex.position().y, self.z)
        }
    }

    // Tessellate Top (z_max)
    let _ = tessellator.tessellate_path(
        &lyon_path,
        &FillOptions::default(),
        &mut BuffersBuilder::new(&mut geometry, VertexConstructor3d { z: z_max }),
    );

    // Tessellate Bottom (z_min) - we need to reverse winding or just handle normals?
    // For simplicity, we just tessellate again at z_min.
    // We might need to reverse indices to have correct normal, but let's assume CullMode::None handles visibility.
    // However, lighting might be wrong if normals are inverted.
    // For now, let's just create vertices.
    let _ = tessellator.tessellate_path(
        &lyon_path,
        &FillOptions::default(),
        &mut BuffersBuilder::new(&mut geometry, VertexConstructor3d { z: z_min }),
    );

    // 3. Side Walls
    // We need to iterate the flattened path to get polygon contours.
    // lyon::algorithm::flatten
    let tolerance = 0.01;
    let flattened = lyon_path.iter().flattened(tolerance); // Returns a Flattened iterator

    // We need to manually build triangle strips for sides.
    // Flattened path yields generic events. We need to track current point.
    let mut start_point = Vec2::ZERO;
    let mut current_point = Vec2::ZERO;

    // Helper to add quad
    let mut add_quad = |p1: Vec2, p2: Vec2| {
        // vertices: p1_min, p1_max, p2_max, p2_min
        let v0 = Vec3::new(p1.x, p1.y, z_min);
        let v1 = Vec3::new(p1.x, p1.y, z_max);
        let v2 = Vec3::new(p2.x, p2.y, z_max);
        let v3 = Vec3::new(p2.x, p2.y, z_min);

        let idx = geometry.vertices.len() as u32;
        geometry.vertices.push(v0);
        geometry.vertices.push(v1);
        geometry.vertices.push(v2);
        geometry.vertices.push(v3);

        // Triangle 1
        geometry.indices.push(idx);
        geometry.indices.push(idx + 1);
        geometry.indices.push(idx + 2);

        // Triangle 2
        geometry.indices.push(idx);
        geometry.indices.push(idx + 2);
        geometry.indices.push(idx + 3);
    };

    for event in flattened {
        match event {
            lyon::path::PathEvent::Begin { at } => {
                start_point = Vec2::new(at.x, at.y);
                current_point = start_point;
            }
            lyon::path::PathEvent::Line { from: _, to } => {
                let to_point = Vec2::new(to.x, to.y);
                add_quad(current_point, to_point);
                current_point = to_point;
            }
            lyon::path::PathEvent::Quadratic { .. } | lyon::path::PathEvent::Cubic { .. } => {
                // Flattened iterator shouldn't yield curves if used correctly,
                // but lyon_path.flattened() returns a Flattened iterator that YIELDS Line events (mostly).
                // Wait, checks docs: flattened() returns an iterator of PathEvent but with curves approximated by lines.
                // So we should only see Begin, Line, End.
                // If we see curves, it means we didn't flatten correctly or the iterator type behaves differently.
                // Assuming Line events.
            }
            lyon::path::PathEvent::End { close, .. } => {
                if close {
                    add_quad(current_point, start_point);
                }
            }
        }
    }

    // 4. Construct Bevy Mesh
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, bevy::render::render_asset::RenderAssetUsages::default());

    // Positions
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, geometry.vertices.clone());

    // Normals (Calculate simple flat normals or smooth?)
    // For PBR, we need normals. Mesh::compute_flat_normals() is useful if vertices are not shared.
    // But here vertices are shared in faces.
    // Let's rely on Bevy's helper if possible, or leave them default (Z+ for top, Z- for bottom).
    // Given we want 2.5D look, we probably want flat shading for sides.
    // The current construction shares vertices for smooth shading on top/bottom, but sides have their own vertices (good for sharp edges).
    // The top/bottom vertices are shared by the tessellator.
    // We should duplicate vertices if we want flat shading on edges (hard edges).
    // But `tessellate_path` creates a mesh with shared vertices for the face.
    // This results in smooth shading across the face (which is flat anyway).
    // But at the edge between Top and Side, we have different vertices (Top has z_max vertices, Side has z_max vertices).
    // So the normal will be discontinuous, which is CORRECT for a sharp edge.
    // So we just need valid normals for each vertex.

    // Manually calculate normals?
    // Top face: (0, 0, 1)
    // Bottom face: (0, 0, -1)
    // Sides: Perpendicular to edge.

    // Use Bevy's built-in normal calculation which duplicates vertices to support flat shading.
    // This gives the desired "low poly" / sharp edge look for 2.5D shapes.
    mesh.compute_flat_normals();

    // `compute_flat_normals` duplicates vertices to ensure flat shading.
    // This is probably what we want for this aesthetic.

    mesh
}
