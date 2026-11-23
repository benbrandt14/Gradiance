//! Shape from an image (brief S-14): one body whose outline follows an
//! image's opaque pixels, with the image on its face.
//!
//! The outline is traced once, at import, through
//! `geometry::polygonize::polygonize_alpha` (the single discretization
//! point) and authored as a plain `Polygon`: no new shape leaf, no format
//! change (decision `image-import-polygon`).

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use bevy::prelude::*;
use gradiance_core::scene_folder::SceneFolder;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::texture::BodyTexture;
use gradiance_geometry::alpha_mask::AlphaMask;
use gradiance_geometry::polygonize::polygonize_alpha;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam};
use gradiance_units::{Displacement, Length};
use serde::Deserialize;
use std::path::Path;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<ImportImage>();
}

/// The opacity of the image file at `path`.
fn read_mask(path: &Path) -> Result<AlphaMask, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension(&extension),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::MAIN_WORLD,
    )
    .map_err(|e| e.to_string())?;
    let (w, h) = (image.width(), image.height());
    let mut alpha = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let a = image.get_color_at(x, y).map_err(|e| e.to_string())?.alpha();
            alpha.push(a);
        }
    }
    AlphaMask::new(w as usize, h as usize, alpha).ok_or_else(|| "an empty image".to_owned())
}

/// Request to pick an image to import (`scene.import_image` with an empty
/// path): the persist layer's dialog answers it by invoking the operation
/// again with the picked path.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ImportImageRequest {
    /// Image centre x (m).
    pub x: f32,
    /// Image centre y (m).
    pub y: f32,
    /// Image width (m).
    pub width: f32,
}

/// Import the image file `path` as one body whose outline follows its
/// opaque pixels, `width` metres wide, centred at (`x`, `y`), with the
/// image on its face.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct ImportImage {
    /// The image file (a PNG with transparency): absolute, or relative to
    /// the scene file's folder. Empty: a file dialog.
    pub path: String,
    /// Image centre x.
    pub x: Length,
    /// Image centre y.
    pub y: Length,
    /// Image width (its height follows the image's aspect).
    pub width: Length,
}

impl Operation for ImportImage {
    const META: OpMeta = OpMeta {
        id: "scene.import_image",
        lisp: "import-image",
        doc: "Import the image file path (a PNG; relative to the scene file's folder; \"\": a file dialog) as one body whose outline follows its opaque pixels (alpha at least 0.5; the largest part), width metres wide, centred at (x, y), with the image on its face — one undo step. Returns its handle.",
        seam: Seam::Edit,
        menu: Some("File/Import image as body…"),
        chord: None,
        undo: Some(crate::intent::name::SPAWN_BODY),
        example: r#"{"path": "assets/images/import-disc.png", "x": 0, "y": 2, "width": 1}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let width = self.width.0;
        if !(width.is_finite() && width > 0.0) {
            return Err(OpError::Invalid(format!(
                "width {width}: a positive length"
            )));
        }
        if self.path.trim().is_empty() {
            return Ok(OpOutput::none().message(ImportImageRequest {
                x: self.x.0,
                y: self.y.0,
                width,
            }));
        }
        let folder = ctx.get::<SceneFolder>().cloned().unwrap_or_default();
        let file = folder.resolve(&self.path);
        let mask =
            read_mask(&file).map_err(|e| OpError::Invalid(format!("{}: {e}", file.display())))?;
        let mut contours = polygonize_alpha(&mask, width)
            .ok_or_else(|| OpError::Invalid(format!("{}: no opaque pixels", file.display())))?;
        // Bodies are centroid-relative; the image stays where it was.
        let centroid = contours.recenter();
        let mut record = super::spawned_body_record(
            ctx.new_id(),
            ShapeDef::Polygon {
                outline: contours.outline,
                holes: contours.holes,
            },
            Vec2::new(self.x.0, self.y.0) + centroid,
        );
        record.texture = Some(BodyTexture {
            offset: Displacement(-centroid),
            width: Length(width),
            ..BodyTexture::new(self.path.clone(), width)
        });
        let handle = record.id.0.to_string();
        Ok(OpOutput::none()
            .message(crate::intent::SpawnBodyIntent { record })
            .returning(handle))
    }
}
