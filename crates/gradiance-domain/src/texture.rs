//! Authored image texture (brief S-13): an image on a body's front face.
//!
//! The image is a file path, not embedded data: the scene stays one small
//! RON file. A relative path resolves against the scene file's folder
//! ([`gradiance_core::scene_folder::SceneFolder`]), else the working
//! directory. Saving does not copy the image (decision `texture-path-only`
//! in `docs/review/archive.ron`).
//!
//! The placement is in the body's frame: the image's centre sits at
//! `offset` from the body origin, it is `width` metres wide (its height
//! follows the image's aspect), and it turns by `angle`. In
//! [`TextureMode::Toon`] the image is the face's albedo, so the look's ramp
//! shades it; in [`TextureMode::Raw`] it shows as it is, unlit.

use bevy::prelude::{Component, Vec2};
use gradiance_units::{Angle, Displacement, Length};
use serde::{Deserialize, Serialize};

/// The narrowest image, in metres.
pub const MIN_TEXTURE_WIDTH: f32 = 0.01;
/// The widest image, in metres.
pub const MAX_TEXTURE_WIDTH: f32 = 1000.0;

/// How the image meets the look.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum TextureMode {
    /// The image is the face's colour; the toon ramp shades it.
    #[default]
    Toon,
    /// The image shows as it is, without lighting.
    Raw,
}

impl TextureMode {
    /// The mode a script names: `"toon"` or `"raw"`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "toon" => Some(Self::Toon),
            "raw" => Some(Self::Raw),
            _ => None,
        }
    }
}

/// An image on a body's front face.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct BodyTexture {
    /// The image file (a PNG): absolute, or relative to the scene
    /// file's folder.
    pub path: String,
    /// The image centre in the body's frame.
    #[serde(default)]
    pub offset: Displacement,
    /// The image width in metres.
    pub width: Length,
    /// The image's turn in the body's frame.
    #[serde(default)]
    pub angle: Angle,
    /// Shaded by the look, or raw.
    #[serde(default)]
    pub mode: TextureMode,
}

impl BodyTexture {
    /// The image at `path`, centred on the body, `width` wide, toon-shaded.
    #[must_use]
    pub fn new(path: impl Into<String>, width: f32) -> Self {
        Self {
            path: path.into(),
            offset: Displacement(Vec2::ZERO),
            width: Length(width),
            angle: Angle(0.0),
            mode: TextureMode::Toon,
        }
    }

    /// The texture with its width held to the range and every number made
    /// finite, or `None` for an empty path (the body keeps no component).
    #[must_use]
    pub fn sanitized(self) -> Option<Self> {
        if self.path.trim().is_empty() {
            return None;
        }
        let width = if self.width.0.is_finite() {
            self.width.0.clamp(MIN_TEXTURE_WIDTH, MAX_TEXTURE_WIDTH)
        } else {
            1.0
        };
        let offset = if self.offset.0.is_finite() {
            self.offset.0
        } else {
            Vec2::ZERO
        };
        let angle = if self.angle.0.is_finite() {
            self.angle.0
        } else {
            0.0
        };
        Some(Self {
            offset: Displacement(offset),
            width: Length(width),
            angle: Angle(angle),
            ..self
        })
    }

    /// The affine map from a body-local point to image coordinates (`u`
    /// right, `v` down, the image spanning `[0, 1]²`), for an image of
    /// `size` pixels: rows `(a, b, c)` with `u = a·p + c` per row.
    #[must_use]
    pub fn local_to_image(&self, size: Vec2) -> [[f32; 3]; 2] {
        let w = self.width.0.max(MIN_TEXTURE_WIDTH);
        let h = w * size.y.max(1.0) / size.x.max(1.0);
        let (sin, cos) = self.angle.0.sin_cos();
        // Turn back by the angle: image-frame x and y of a local point.
        let ex = Vec2::new(cos, sin);
        let ey = Vec2::new(-sin, cos);
        let o = self.offset.0;
        // u = ex·(p - o)/w + 1/2, v = 1/2 - ey·(p - o)/h.
        [
            [ex.x / w, ex.y / w, 0.5 - ex.dot(o) / w],
            [-ey.x / h, -ey.y / h, 0.5 + ey.dot(o) / h],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: [[f32; 3]; 2], p: Vec2) -> Vec2 {
        Vec2::new(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2],
            m[1][0] * p.x + m[1][1] * p.y + m[1][2],
        )
    }

    #[test]
    fn the_image_corners_land_on_its_placement() {
        // A 200 x 100 px image, 2 m wide, so 1 m high, centred at (1, 0).
        let mut t = BodyTexture::new("a.png", 2.0);
        t.offset = Displacement(Vec2::new(1.0, 0.0));
        let m = t.local_to_image(Vec2::new(200.0, 100.0));
        assert!(apply(m, Vec2::new(1.0, 0.0)).abs_diff_eq(Vec2::splat(0.5), 1e-6));
        // Top-left corner: u = 0, v = 0.
        assert!(apply(m, Vec2::new(0.0, 0.5)).abs_diff_eq(Vec2::ZERO, 1e-6));
        assert!(apply(m, Vec2::new(2.0, -0.5)).abs_diff_eq(Vec2::ONE, 1e-6));
        // A quarter turn: the image's right edge points up.
        t.angle = Angle(core::f32::consts::FRAC_PI_2);
        let m = t.local_to_image(Vec2::new(200.0, 100.0));
        assert!(apply(m, Vec2::new(1.0, 1.0)).abs_diff_eq(Vec2::new(1.0, 0.5), 1e-6));
    }

    #[test]
    fn an_empty_path_is_none_and_bad_numbers_are_held() {
        assert_eq!(BodyTexture::new(" ", 1.0).sanitized(), None);
        let t = BodyTexture {
            width: Length(f32::NAN),
            angle: Angle(f32::INFINITY),
            ..BodyTexture::new("a.png", 1.0)
        }
        .sanitized();
        let t = t.unwrap_or_else(|| BodyTexture::new("", 0.0));
        assert_eq!((t.width.0, t.angle.0), (1.0, 0.0));
        assert_eq!(
            BodyTexture::new("a.png", 1e9)
                .sanitized()
                .map(|t| t.width.0),
            Some(MAX_TEXTURE_WIDTH)
        );
        assert_eq!(TextureMode::named("raw"), Some(TextureMode::Raw));
        assert_eq!(TextureMode::named("x"), None);
    }
}
