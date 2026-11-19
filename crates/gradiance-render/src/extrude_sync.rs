//! Authored shape + layers → derived extruded `Mesh3d`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::extrusion::extrude_contours;
use gradiance_geometry::face_field::{FaceField, face_field};
use gradiance_geometry::polygonize::polygonize;

/// Builds the 2.5D prism mesh for a body (pure; unit-tested headless).
///
/// Half-planes are the exception: they render as an infinite flat *floor*
/// (a plane containing the surface line, sweeping along ±Z) rather than a
/// prism — the scene reads as pieces standing on a 3D surface. See
/// `render::plane` for the material that completes the impression.
pub fn build_body_mesh(shape: &ShapeDef, band: &DepthBand) -> Mesh {
    if matches!(shape, ShapeDef::HalfPlane) {
        return crate::plane::ground_plane_mesh();
    }
    mesh_of(&polygonize(shape), *band)
}

/// The prism mesh for already polygonized `contours`.
fn mesh_of(contours: &gradiance_geometry::contours::Contours, band: DepthBand) -> Mesh {
    let band = band.sanitized();
    let buffers = extrude_contours(contours, band.z_front(), band.thickness());

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, buffers.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, buffers.normals);
    // Face coordinates, not a texture mapping: `toon.wesl` bends the front
    // cap's normal over them (relief and glint, brief 4b-5).
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, buffers.uvs);
    mesh.insert_indices(Indices::U32(buffers.indices));
    mesh
}

/// A face field as a GPU image: linear (not sRGB: the bytes are
/// directions and depths, not colours), bilinear, clamped at the edges.
pub fn face_field_image(field: &FaceField) -> Image {
    use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut image = Image::new(
        Extent3d {
            width: field.width,
            height: field.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        field.bytes(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

/// Regenerates the extruded mesh, and the face field the toon shader rounds
/// the front face with (brief 4b-6), when a body's shape or layers change.
pub fn sync_body_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    changed: Query<
        (Entity, &ShapeDef, &DepthBand),
        (With<Body>, Or<(Changed<ShapeDef>, Changed<DepthBand>)>),
    >,
) {
    for (entity, shape, band) in &changed {
        let mut body = commands.entity(entity);
        if matches!(shape, ShapeDef::HalfPlane) {
            body.insert(Mesh3d(meshes.add(build_body_mesh(shape, band))));
            continue;
        }
        let contours = polygonize(shape);
        body.insert((
            Mesh3d(meshes.add(mesh_of(&contours, *band))),
            crate::toon::FaceFieldImage(images.add(face_field_image(&face_field(&contours)))),
        ));
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
        let band = DepthBand::metres(10.0, 30.0); // z ∈ [-30, -10]
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
