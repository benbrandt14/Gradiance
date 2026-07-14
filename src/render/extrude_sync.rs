//! Authored shape + layers → derived extruded `Mesh3d`.

use crate::domain::Body;
use crate::domain::depth::DepthBand;
use crate::domain::shape::ShapeDef;
use crate::geometry::extrusion::extrude_contours;
use crate::geometry::polygonize::polygonize;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Builds the 2.5D prism mesh for a body (pure; unit-tested headless).
///
/// Half-planes are the exception: they render as an infinite flat *floor*
/// (a plane containing the surface line, sweeping along ±Z) rather than a
/// prism — the scene reads as pieces standing on a 3D surface. See
/// `render::plane` for the material that completes the impression.
pub fn build_body_mesh(shape: &ShapeDef, band: &DepthBand) -> Mesh {
    if matches!(shape, ShapeDef::HalfPlane) {
        return crate::render::plane::ground_plane_mesh();
    }
    let band = band.sanitized();
    let buffers = extrude_contours(&polygonize(shape), band.z_front(), band.thickness());

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
        (Entity, &ShapeDef, &DepthBand),
        (With<Body>, Or<(Changed<ShapeDef>, Changed<DepthBand>)>),
    >,
) {
    for (entity, shape, band) in &changed {
        let mesh = build_body_mesh(shape, band);
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
        let band = DepthBand {
            near: 10.0,
            far: 30.0, // z ∈ [-30, -10]
        };
        let mesh = build_body_mesh(&shape, &band);
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
