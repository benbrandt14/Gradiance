//! Texture operations (brief S-13): put an image on a body's front face,
//! place it, pick its mode. Each is one undo step through the property
//! edit seam.

use super::bodies_of;
use crate::intent::{PropertyEditIntent, name};
use crate::property::{PropertyChange, PropertyValue};
use crate::snapshot::{BodySnapshot, SceneSnapshot};
use bevy::prelude::*;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::texture::{BodyTexture, TextureMode};
use gradiance_geometry::extrusion::face_frame;
use gradiance_geometry::polygonize::polygonize;
use gradiance_op::{OpContext, OpError, OpMeta, OpOutput, OpRegistry, Operation, Seam, Target};
use gradiance_units::{Angle, Displacement, Length};
use serde::Deserialize;

/// Registers this file's operations.
pub(super) fn register(registry: &mut OpRegistry) {
    registry.register::<SetTexture>();
    registry.register::<PlaceTexture>();
    registry.register::<SetTextureMode>();
}

/// One property edit per body whose texture `edit` changes.
fn texture_edits(
    ctx: &OpContext,
    target: &Target,
    edit: impl Fn(&BodySnapshot) -> Option<BodyTexture>,
) -> Result<OpOutput, OpError> {
    let scene = ctx.get::<SceneSnapshot>()?;
    let changes: Vec<PropertyChange> = bodies_of(ctx, scene, target)?
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|body| !body.shape.contains_half_plane())
        .filter_map(|body| {
            let new = edit(body).and_then(BodyTexture::sanitized);
            (new != body.texture).then(|| PropertyChange {
                id: body.id,
                old: PropertyValue::Texture(body.texture.clone()),
                new: PropertyValue::Texture(new),
            })
        })
        .collect();
    if changes.is_empty() {
        return Ok(OpOutput::none());
    }
    Ok(OpOutput::none().message(PropertyEditIntent { changes }))
}

/// A body's texture after its path is set to `path`: an image already
/// there keeps its placement; a new one is centred on the face and as wide
/// as it; an empty path removes the image. The one rule the verb and the
/// inspector share.
#[must_use]
pub fn with_path(old: Option<&BodyTexture>, shape: &ShapeDef, path: &str) -> Option<BodyTexture> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(if let Some(t) = old {
        BodyTexture {
            path: path.to_owned(),
            ..t.clone()
        }
    } else {
        let (centre, half) = face_frame(&polygonize(shape));
        BodyTexture {
            offset: Displacement(centre),
            ..BodyTexture::new(path, 2.0 * half.x)
        }
    })
}

/// Show the image file `path` on the i-th body's front face (`""` removes
/// it). A new image covers the face's width; an image already there keeps
/// its placement.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetTexture {
    /// The body.
    pub i: Target,
    /// The image file (a PNG): absolute, or relative to the scene
    /// file's folder; `""` removes the image.
    pub path: String,
}

impl Operation for SetTexture {
    const META: OpMeta = OpMeta {
        id: "body.set_texture",
        lisp: "set-texture",
        doc: "Show the image file path (a PNG; relative to the scene file's folder) on the i-th body's front face, as wide as the face, shaded by the look; \"\" removes it — undoable, saved (the path, not the image).",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 0, "path": "images/logo.png"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        texture_edits(ctx, &self.i, |body| {
            with_path(body.texture.as_ref(), &body.shape, &self.path)
        })
    }
}

/// Place the i-th body's image: its centre at (`x`, `y`) in the body's
/// frame, `width` metres wide, turned by `angle`.
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct PlaceTexture {
    /// The body.
    pub i: Target,
    /// Image centre, body frame, x.
    pub x: Length,
    /// Image centre, body frame, y.
    pub y: Length,
    /// Image width (its height follows the image's aspect).
    pub width: Length,
    /// Turn in the body's frame.
    pub angle: Angle,
}

impl Operation for PlaceTexture {
    const META: OpMeta = OpMeta {
        id: "body.place_texture",
        lisp: "place-texture",
        doc: "Place the i-th body's image: centre (x, y) in the body's frame, width metres wide, turned by angle — undoable. A body without an image is left as it is.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 5, "x": 0.1, "y": 0, "width": 0.5, "angle": 0.3}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        texture_edits(ctx, &self.i, |body| {
            body.texture.clone().map(|t| BodyTexture {
                offset: Displacement(Vec2::new(self.x.0, self.y.0)),
                width: self.width,
                angle: self.angle,
                ..t
            })
        })
    }
}

/// Shade the i-th body's image by the look (`"toon"`) or show it as it is
/// (`"raw"`).
#[derive(Debug, Deserialize, Reflect)]
#[serde(deny_unknown_fields)]
pub struct SetTextureMode {
    /// The body.
    pub i: Target,
    /// `"toon"` or `"raw"`.
    pub mode: String,
}

impl Operation for SetTextureMode {
    const META: OpMeta = OpMeta {
        id: "body.set_texture_mode",
        lisp: "set-texture-mode",
        doc: "Shade the i-th body's image by the look (\"toon\") or show it unlit as it is (\"raw\") — undoable. A body without an image is left as it is.",
        seam: Seam::Edit,
        menu: None,
        chord: None,
        undo: Some(name::PROPERTY_EDIT),
        example: r#"{"i": 5, "mode": "raw"}"#,
    };

    fn lower(self, ctx: &OpContext) -> Result<OpOutput, OpError> {
        let mode = TextureMode::named(&self.mode).ok_or_else(|| {
            OpError::Invalid(format!("mode {:?}: \"toon\" or \"raw\"", self.mode))
        })?;
        texture_edits(ctx, &self.i, |body| {
            body.texture.clone().map(|t| BodyTexture { mode, ..t })
        })
    }
}
