//! Mesh generation for 3D shapes.

use crate::input::editable_shape::ShapeType;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};
use lyon::math::Point;
use lyon::path::Path;

struct SimpleVertexConstructor;

impl FillVertexConstructor<[f32; 3]> for SimpleVertexConstructor {
    fn new_vertex(&mut self, vertex: FillVertex) -> [f32; 3] {
        [vertex.position().x, vertex.position().y, 0.0]
    }
}

/// Generates a 3D mesh for the given shape type.
/// The mesh is oriented in the XY plane (normal along Z).
pub fn generate_3d_mesh(shape: &ShapeType) -> Mesh {
    match shape {
        ShapeType::Box { width, height } => {
            Mesh::from(Cuboid::new(*width, *height, 1.0)) // Depth 1.0
        }
        ShapeType::Circle { radius } => {
            let mut mesh = Mesh::from(Cylinder::new(*radius, 1.0));
            // Rotate mesh 90 degrees around X to align cylinder cap with XY plane
            let rotation = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
            rotate_mesh(&mut mesh, rotation);
            mesh
        }
        ShapeType::Polygon { points } => {
            let mut buffers: VertexBuffers<[f32; 3], u32> = VertexBuffers::new();
            let mut tessellator = FillTessellator::new();
            let options = FillOptions::default();

            // Build Lyon Path
            let mut builder = Path::builder();
            if let Some(first) = points.first() {
                builder.begin(Point::new(first.x, first.y));
                for p in points.iter().skip(1) {
                    builder.line_to(Point::new(p.x, p.y));
                }
                builder.close();
            }
            let path = builder.build();

            if tessellator
                .tessellate_path(
                    &path,
                    &options,
                    &mut BuffersBuilder::new(&mut buffers, SimpleVertexConstructor),
                )
                .is_ok()
            {
                let mut mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                );

                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.vertices);
                mesh.insert_indices(Indices::U32(buffers.indices));

                // Add normals (all pointing +Z)
                let count = mesh.count_vertices();
                let normals = vec![[0.0, 0.0, 1.0]; count];
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);

                mesh
            } else {
                warn!("Failed to tessellate polygon");
                Mesh::from(Cuboid::new(1.0, 1.0, 1.0))
            }
        }
    }
}

fn rotate_mesh(mesh: &mut Mesh, rotation: Quat) {
    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for pos in positions.iter_mut() {
            let vec = rotation * Vec3::from(*pos);
            *pos = [vec.x, vec.y, vec.z];
        }
    }

    if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for normal in normals.iter_mut() {
            let vec = rotation * Vec3::from(*normal);
            *normal = [vec.x, vec.y, vec.z];
        }
    }
}
