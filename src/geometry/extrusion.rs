//! 2.5D Extrusion Logic.
//!
//! Handles generating 3D meshes from 2D paths based on collision layers.

use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy_prototype_lyon::prelude::*; // For Path component wrapper
use bevy_rapier2d::prelude::*;
use lyon::path::PathEvent;
use lyon::path::iterator::PathIterator;
use lyon::tessellation::{self, BuffersBuilder, FillOptions, FillTessellator, VertexBuffers}; // Import trait for flattened

/// Plugin that registers the extrusion component and logic.
pub struct ExtrusionPlugin;

impl Plugin for ExtrusionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ExtrudableShape>();
        app.add_systems(Update, update_extrusion_mesh);
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

fn generate_mesh_hook(
    mut world: DeferredWorld,
    entity: Entity,
    _component_id: bevy::ecs::component::ComponentId,
) {
    let path = world.get::<Path>(entity).map(|p| p.0.clone());
    let groups = world.get::<CollisionGroups>(entity).copied();

    let Some(path) = path else {
        warn!("ExtrudableShape added to entity {:?} without Path", entity);
        return;
    };

    let groups = groups.unwrap_or(CollisionGroups::default());

    let (mesh, material) = create_extruded_mesh(&path, groups);

    // Asset Registration
    let mesh_handle = {
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        meshes.add(mesh)
    };

    let material_handle = {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        materials.add(material)
    };

    // Safe Component Insertion to prevent panic if entity is despawned
    world.commands().queue(move |world: &mut World| {
        if world.get_entity(entity).is_ok()
            && let Ok(mut entity_mut) = world.get_entity_mut(entity)
        {
            entity_mut.insert((Mesh3d(mesh_handle), MeshMaterial3d(material_handle)));
        }
    });
}

fn update_extrusion_mesh(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (
            Entity,
            &Path,
            &CollisionGroups,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
        ),
        (
            With<ExtrudableShape>,
            Or<(
                Changed<CollisionGroups>,
                Changed<Path>,
                Added<CollisionGroups>,
            )>,
        ),
    >,
) {
    for (entity, path, groups, mesh_handle, mat_handle) in query.iter() {
        let (mesh, material) = create_extruded_mesh(&path.0, *groups);

        // Remove old assets to prevent leaks
        meshes.remove(&mesh_handle.0);
        materials.remove(&mat_handle.0);

        let new_mesh_handle = meshes.add(mesh);
        let new_mat_handle = materials.add(material);

        commands
            .entity(entity)
            .insert((Mesh3d(new_mesh_handle), MeshMaterial3d(new_mat_handle)));
    }
}

fn create_extruded_mesh(
    path: &lyon::path::Path,
    groups: CollisionGroups,
) -> (Mesh, StandardMaterial) {
    // Parse memberships
    let layer_h = 10.0;
    let memberships = groups.memberships.bits();

    let mut min_i = 32;
    let mut max_i = 0;
    let mut active = false;

    for i in 0..32 {
        if (memberships >> i) & 1 == 1 {
            if i < min_i {
                min_i = i;
            }
            if i > max_i {
                max_i = i;
            }
            active = true;
        }
    }

    if !active {
        return (
            Mesh::new(
                PrimitiveTopology::TriangleList,
                bevy::render::render_asset::RenderAssetUsages::default(),
            ),
            StandardMaterial::default(),
        );
    }

    let z_front = -(min_i as f32 * layer_h);
    let depth = (max_i - min_i + 1) as f32 * layer_h;
    let z_back = z_front - depth;

    // Mesh Data Arrays
    let mut positions: Vec<Vec3> = Vec::new();
    let mut normals: Vec<Vec3> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // 1. Tessellate Front Cap (z_front)
    {
        let mut buffers: VertexBuffers<Vec3, u32> = VertexBuffers::new();
        let mut tessellator = FillTessellator::new();

        struct VertexConstructor {
            z: f32,
        }
        impl tessellation::FillVertexConstructor<Vec3> for VertexConstructor {
            fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> Vec3 {
                let p = vertex.position();
                Vec3::new(p.x, p.y, self.z)
            }
        }

        if tessellator
            .tessellate_path(
                path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(&mut buffers, VertexConstructor { z: z_front }),
            )
            .is_ok()
        {
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
    {
        let mut buffers: VertexBuffers<Vec3, u32> = VertexBuffers::new();
        let mut tessellator = FillTessellator::new();

        struct VertexConstructor {
            z: f32,
        }
        impl tessellation::FillVertexConstructor<Vec3> for VertexConstructor {
            fn new_vertex(&mut self, vertex: tessellation::FillVertex) -> Vec3 {
                let p = vertex.position();
                Vec3::new(p.x, p.y, self.z)
            }
        }

        if tessellator
            .tessellate_path(
                path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(&mut buffers, VertexConstructor { z: z_back }),
            )
            .is_ok()
        {
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

                add_quad(
                    p1,
                    p2,
                    z_back,
                    z_front,
                    &mut positions,
                    &mut normals,
                    &mut indices,
                );
                current_point = p2;
            }
            PathEvent::End {
                last: _,
                first: _,
                close,
            } => {
                if close {
                    let p1 = current_point;
                    let p2 = start_point;
                    add_quad(
                        p1,
                        p2,
                        z_back,
                        z_front,
                        &mut positions,
                        &mut normals,
                        &mut indices,
                    );
                }
            }
            _ => {}
        }
    }

    // Build Mesh
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::render::render_asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));

    // Material
    // Color based on layer index (min_i) to distinguish zones
    let hue = min_i as f32 * 30.0;
    let color = Color::hsl(hue, 0.8, 0.5);
    let material = StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.5,
        metallic: 0.0,
        // Disabled culling to ensure visibility
        cull_mode: None,
        double_sided: true,
        ..default()
    };

    (mesh, material)
}

fn point_to_vec2(p: lyon::math::Point) -> Vec2 {
    Vec2::new(p.x, p.y)
}

fn add_quad(
    p1: Vec2,
    p2: Vec2,
    z_back: f32,
    z_front: f32,
    positions: &mut Vec<Vec3>,
    normals: &mut Vec<Vec3>,
    indices: &mut Vec<u32>,
) {
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

fn add_vertex_internal(
    pos: Vec3,
    normal: Vec3,
    positions: &mut Vec<Vec3>,
    normals: &mut Vec<Vec3>,
) -> u32 {
    positions.push(pos);
    normals.push(normal);
    (positions.len() - 1) as u32
}
