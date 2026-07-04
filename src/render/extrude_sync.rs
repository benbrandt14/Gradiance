//! Authored shape + layers → derived extruded `Mesh3d`.

use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use crate::geometry::extrusion::{extrude_contours, layer_z_range};
use crate::geometry::polygonize::polygonize;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Builds the 2.5D prism mesh for a body (pure; unit-tested headless).
pub fn build_body_mesh(shape: &ShapeDef, layers: &LayerMask32) -> Mesh {
    let (min_bit, max_bit) = layers.occupied_range().unwrap_or((0, 0));
    let (z_front, depth) = layer_z_range(min_bit, max_bit);
    let buffers = extrude_contours(&polygonize(shape), z_front, depth);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, buffers.normals);
    mesh.insert_indices(Indices::U32(buffers.indices));
    mesh
}

/// Regenerates the extruded mesh when a body's shape or layers change.
pub fn sync_body_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    changed: Query<
        (Entity, &ShapeDef, &LayerMask32),
        (With<Body>, Or<(Changed<ShapeDef>, Changed<LayerMask32>)>),
    >,
) {
    for (entity, shape, layers) in &changed {
        let mesh = build_body_mesh(shape, layers);
        commands.entity(entity).insert(Mesh3d(meshes.add(mesh)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_mesh_z_range_follows_layers() {
        let shape = ShapeDef::Box {
            width: 20.0,
            height: 10.0,
        };
        let layers = LayerMask32 {
            memberships: 0b0110, // bits 1..=2 → z ∈ [-30, -10]
            filters: u32::MAX,
        };
        let mesh = build_body_mesh(&shape, &layers);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("positions");
        let (mut zmin, mut zmax) = (f32::MAX, f32::MIN);
        for p in positions {
            zmin = zmin.min(p[2]);
            zmax = zmax.max(p[2]);
        }
        assert!((zmax + 10.0).abs() < 1e-4, "front at -10 (zmax {zmax})");
        assert!((zmin + 30.0).abs() < 1e-4, "back at -30 (zmin {zmin})");
    }
}
