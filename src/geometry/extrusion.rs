//! 2.5D Extrusion Logic.
//!
//! Handles generating 3D meshes from 2D paths based on collision layers.

use bevy::prelude::*;
use bevy::ecs::world::DeferredWorld;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy_prototype_lyon::prelude::*; // For Path component wrapper
use bevy_rapier2d::prelude::*;
use lyon::path::PathEvent;
use lyon::tessellation::{
    self, BuffersBuilder, FillOptions, FillTessellator, VertexBuffers,
};
use lyon::path::iterator::PathIterator; // Import trait for flattened

/// Plugin that registers the extrusion component and logic.
pub struct ExtrusionPlugin;

impl Plugin for ExtrusionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ExtrudableShape>();
    }
}

/// A component that marks an entity for 3D extrusion.
///
/// When added, it triggers a hook that generates a `Mesh3d` and `MeshMaterial3d`
/// based on the entity's `Path` and `CollisionGroups`.
#[derive(Component, Reflect, Default, Debug)]
#[reflect(Component)]
#[require(Mesh3d, MeshMaterial3d<StandardMaterial>)]
#[component(on_add = generate_mesh_hook)]
pub struct ExtrudableShape;

fn generate_mesh_hook(mut world: DeferredWorld, entity: Entity, _component_id: bevy::ecs::component::ComponentId) {
    let path = world.get::<Path>(entity).map(|p| p.0.clone());
    let groups = world.get::<CollisionGroups>(entity).copied();

    let Some(path) = path else {
        warn!("ExtrudableShape added to entity {:?} without Path", entity);
        return;
    };

    let groups = groups.unwrap_or(CollisionGroups::default());

    // Parse memberships
    let layer_h = 10.0;
    let memberships = groups.memberships.bits();

    let mut min_i = 32;
    let mut max_i = 0;
    let mut active = false;

    for i in 0..32 {
        if (memberships >> i) & 1 == 1 {
            if i < min_i { min_i = i; }
            if i > max_i { max_i = i; }
            active = true;
        }
    }

    if !active {
        min_i = 0;
        max_i = 0;
    }

    // Spec: "physics plane should be in the 'front' so debug objects are visualized over the extrusion".
    // Physics world is at Z=0.
    // So Extrusion must be BEHIND Z=0 (Negative Z).
    // Original: `z_start = min_i * layer_h`. Extruded to `z_start + depth`. (Positive Z).
    // New:
    // We want Layer 0 to be just behind Z=0.
    // Layer 0: Z range [-10, 0].
    // Layer 1: Z range [-20, -10].
    // min_i maps to the "Front" of the object.
    // So `z_front = - (min_i * layer_h)`.
    // `z_back = z_front - depth`.

    // Let's re-verify.
    // If multiple layers active (e.g. 0 and 1).
    // min_i = 0. max_i = 1.
    // depth = (1 - 0 + 1) * 10 = 20.
    // z_front (at Z=0) = 0.
    // z_back = -20.
    // Object spans [-20, 0].
    // Debug lines at Z=0 are visible on top. Correct.

    // What if min_i = 1?
    // z_front = -1 * 10 = -10.
    // Object starts at -10. Correct.

    let z_front = -(min_i as f32 * layer_h);
    let depth = (max_i as i32 - min_i as i32 + 1) as f32 * layer_h;
    let z_back = z_front - depth;

    // Mesh Data Arrays
    let mut positions: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // 1. Tessellate Front Cap (z_front)
    // Front face: Normal (0,0,1). Winding CCW.
    {
        let mut buffers: VertexBuffers<Vec3, u32> = VertexBuffers::new();
        let mut tessellator = FillTessellator::new();

        struct VertexConstructor { z: f32 }
        impl tessellation::FillVertexConstructor<Vec3> for VertexConstructor {
            fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> Vec3 {
                let p = vertex.position();
                Vec3::new(p.x, p.y, self.z)
            }
        }

        if tessellator.tessellate_path(
            &path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut buffers, VertexConstructor { z: z_front })
        ).is_ok() {
            let base_idx = positions.len() as u32;
            for p in buffers.vertices {
                positions.push(p);
                normals.push(Vec3::Z);
            }
            for i in buffers.indices {
                indices.push(base_idx + i);
            }
        }
    }

    // 2. Tessellate Back Cap (z_back)
    // Back face: Normal (0,0,-1). Winding CW.
    {
        let mut buffers: VertexBuffers<Vec3, u32> = VertexBuffers::new();
        let mut tessellator = FillTessellator::new();

        struct VertexConstructor { z: f32 }
        impl tessellation::FillVertexConstructor<Vec3> for VertexConstructor {
            fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> Vec3 {
                let p = vertex.position();
                Vec3::new(p.x, p.y, self.z)
            }
        }

        if tessellator.tessellate_path(
            &path,
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut buffers, VertexConstructor { z: z_back })
        ).is_ok() {
            let base_idx = positions.len() as u32;
            for p in buffers.vertices {
                positions.push(p);
                normals.push(-Vec3::Z);
            }
            // Reverse indices for back face
            for i in buffers.indices.chunks(3) {
                if i.len() == 3 {
                    indices.push(base_idx + i[2]);
                    indices.push(base_idx + i[1]);
                    indices.push(base_idx + i[0]);
                }
            }
        }
    }

    // 3. Side Walls
    let tolerance = 0.05;
    let flattened = path.iter().flattened(tolerance);

    let mut current_point = Vec2::ZERO;
    let mut first_point = true;
    let mut start_point = Vec2::ZERO;

    for event in flattened {
        match event {
            PathEvent::Begin { at } => {
                start_point = point_to_vec2(at);
                current_point = start_point;
                first_point = true;
            }
            PathEvent::Line { from: _, to } => {
                if first_point {
                    first_point = false;
                }
                let p1 = current_point;
                let p2 = point_to_vec2(to);

                // Add quad between z_back and z_front.
                // Note order: z_back is "far", z_front is "near".
                // add_quad expects (z_start, z_end) where normal calculation assumes direction?
                // `add_quad` logic assumed `z_start` is back and `z_end` is front.
                // Here `z_back` is smaller than `z_front`.
                // So pass `z_back` as start, `z_front` as end?
                // Let's check `add_quad`.

                add_quad(p1, p2, z_back, z_front, &mut positions, &mut normals, &mut indices);
                current_point = p2;
            }
            PathEvent::End { last: _, first: _, close } => {
                if close {
                    let p1 = current_point;
                    let p2 = start_point;
                    add_quad(p1, p2, z_back, z_front, &mut positions, &mut normals, &mut indices);
                }
            }
            _ => {}
        }
    }

    // Build Mesh
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, bevy::render::render_asset::RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));

    // Asset Registration
    let mesh_handle = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        meshes.add(mesh)
    };

    let material_handle = {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        // Color based on layer index (min_i) to distinguish zones
        let hue = min_i as f32 * 30.0;
        let color = Color::hsl(hue, 0.8, 0.5);
        materials.add(StandardMaterial {
            base_color: color,
            perceptual_roughness: 0.5,
            metallic: 0.0,
            double_sided: true,
            ..default()
        })
    };

    // Safe Component Insertion to prevent panic if entity is despawned
    world.commands().queue(move |world: &mut World| {
        if world.get_entity(entity).is_ok() {
            if let Ok(mut entity_mut) = world.get_entity_mut(entity) {
                entity_mut.insert((
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material_handle),
                ));
            }
        }
    });
}

fn point_to_vec2(p: lyon::math::Point) -> Vec2 {
    Vec2::new(p.x, p.y)
}

fn add_quad(p1: Vec2, p2: Vec2, z_back: f32, z_front: f32, positions: &mut Vec<Vec3>, normals: &mut Vec<Vec3>, indices: &mut Vec<u32>)
{
    // Tangent = p2 - p1. Normal = (tangent.y, -tangent.x).
    let tangent = p2 - p1;
    let normal2d = Vec2::new(tangent.y, -tangent.x).normalize_or_zero();
    let normal = Vec3::new(normal2d.x, normal2d.y, 0.0);

    // Vertices
    // i0: p1, z_back
    // i1: p2, z_back
    // i2: p2, z_front
    // i3: p1, z_front

    let i0 = add_vertex_internal(Vec3::new(p1.x, p1.y, z_back), normal, positions, normals);
    let i1 = add_vertex_internal(Vec3::new(p2.x, p2.y, z_back), normal, positions, normals);
    let i2 = add_vertex_internal(Vec3::new(p2.x, p2.y, z_front), normal, positions, normals);
    let i3 = add_vertex_internal(Vec3::new(p1.x, p1.y, z_front), normal, positions, normals);

    // Triangles
    // CCW Order (assuming looking from outside right):
    // 0 -> 1 -> 2
    // 2 -> 3 -> 0

    indices.push(i0);
    indices.push(i1);
    indices.push(i2);

    indices.push(i2);
    indices.push(i3);
    indices.push(i0);
}

fn add_vertex_internal(pos: Vec3, normal: Vec3, positions: &mut Vec<Vec3>, normals: &mut Vec<Vec3>) -> u32 {
    positions.push(pos);
    normals.push(normal);
    (positions.len() - 1) as u32
}
