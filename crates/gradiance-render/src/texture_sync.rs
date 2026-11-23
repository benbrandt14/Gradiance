//! Body images (brief S-13): reads each [`BodyTexture`]'s file once, and
//! hands the image and its placement to the body's toon material.
//!
//! Derived state: the decoded image lives in `Assets<Image>` and a cache by
//! resolved path; nothing here is saved. A file that cannot be read or
//! decoded logs one warning and the face shows its fill.

use crate::toon::ToonMaterial;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use gradiance_core::scene_folder::SceneFolder;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::texture::{BodyTexture, TextureMode};
use gradiance_geometry::extrusion::face_frame;
use gradiance_geometry::polygonize::polygonize;
use std::path::{Path, PathBuf};

/// Decoded images by resolved path, with their size in pixels; `None` for
/// a file that failed (so it is not read again every frame).
#[derive(Resource, Default)]
pub struct TextureCache(HashMap<PathBuf, Option<(Handle<Image>, Vec2)>>);

impl TextureCache {
    /// The image at `path`, read and decoded on first use.
    fn get(&mut self, path: &Path, images: &mut Assets<Image>) -> Option<(Handle<Image>, Vec2)> {
        self.0
            .entry(path.to_path_buf())
            .or_insert_with(|| match decode(path) {
                Ok(image) => {
                    let size = image.size_f32();
                    Some((images.add(image), size))
                }
                Err(error) => {
                    warn!(path = %path.display(), %error, "body image not loaded");
                    None
                }
            })
            .clone()
    }
}

/// Reads and decodes one image file (sRGB).
fn decode(path: &Path) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    Image::from_buffer(
        &bytes,
        ImageType::Extension(&extension),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|e| e.to_string())
}

/// The material rows for `texture` on a face of `shape`, for an image of
/// `size` pixels: the map from face coordinates (the outline's box spans
/// `[-1, 1]²`) to image coordinates, and the flags.
#[must_use]
pub fn image_rows(texture: &BodyTexture, shape: &ShapeDef, size: Vec2) -> (Vec4, Vec4) {
    let (centre, half) = face_frame(&polygonize(shape));
    let m = texture.local_to_image(size);
    // p = centre + uv * half, so u = m·p + c = (m * half)·uv + (m·centre + c).
    let row = |r: [f32; 3], flag: f32| {
        Vec4::new(
            r[0] * half.x,
            r[1] * half.y,
            r[0] * centre.x + r[1] * centre.y + r[2],
            flag,
        )
    };
    let raw = if texture.mode == TextureMode::Raw {
        1.0
    } else {
        0.0
    };
    (row(m[0], 1.0), row(m[1], raw))
}

/// Hands each body's image to its material when the texture, the shape or
/// the material changes, and takes it away when the texture goes.
#[allow(clippy::type_complexity)]
pub fn attach_textures(
    mut cache: ResMut<TextureCache>,
    folder: Option<Res<SceneFolder>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    textured: Query<
        (&BodyTexture, &ShapeDef, &MeshMaterial3d<ToonMaterial>),
        Or<(
            Changed<BodyTexture>,
            Changed<ShapeDef>,
            Changed<MeshMaterial3d<ToonMaterial>>,
        )>,
    >,
    plain: Query<&MeshMaterial3d<ToonMaterial>, Without<BodyTexture>>,
    mut removed: RemovedComponents<BodyTexture>,
) {
    for entity in removed.read() {
        if let Ok(material) = plain.get(entity)
            && let Some(mut material) = materials.get_mut(&material.0)
        {
            material.extension.image = None;
            material.extension.image_u = Vec4::ZERO;
            material.extension.image_v = Vec4::ZERO;
        }
    }
    let folder = folder.map_or_else(SceneFolder::default, |f| f.clone());
    for (texture, shape, material) in &textured {
        let loaded = cache.get(&folder.resolve(&texture.path), &mut images);
        let Some(mut material) = materials.get_mut(&material.0) else {
            continue;
        };
        if let Some((image, size)) = loaded {
            let (u, v) = image_rows(texture, shape, size);
            material.extension.image = Some(image);
            material.extension.image_u = u;
            material.extension.image_v = v;
        } else {
            material.extension.image = None;
            material.extension.image_u = Vec4::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_face_corners_map_onto_an_image_that_covers_the_face() {
        // A 2 x 1 box; an image as wide as the face, centred, 2:1 aspect.
        let shape = ShapeDef::Box {
            width: 2.0,
            height: 1.0,
        };
        let texture = BodyTexture::new("a.png", 2.0);
        let (u, v) = image_rows(&texture, &shape, Vec2::new(200.0, 100.0));
        let at = |uv: Vec2| Vec2::new(u.xy().dot(uv) + u.z, v.xy().dot(uv) + v.z);
        assert!(
            at(Vec2::new(-1.0, 1.0)).abs_diff_eq(Vec2::ZERO, 1e-5),
            "top left"
        );
        assert!(
            at(Vec2::new(1.0, -1.0)).abs_diff_eq(Vec2::ONE, 1e-5),
            "bottom right"
        );
        assert_eq!((u.w, v.w), (1.0, 0.0), "bound, toon");
    }
}
