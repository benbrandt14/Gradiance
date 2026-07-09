//! Authored visual styling of a body.

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};

/// A plain linear-space RGBA color, serialization-friendly.
///
/// Deliberately not Bevy's `Color` so the save format stays decoupled from
/// engine color-space enums; the render layer converts on sync.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct Rgba {
    /// Red in `[0, 1]`.
    pub r: f32,
    /// Green in `[0, 1]`.
    pub g: f32,
    /// Blue in `[0, 1]`.
    pub b: f32,
    /// Alpha in `[0, 1]`.
    pub a: f32,
}

impl Rgba {
    /// Builds an opaque color.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Builds a color from HSL (hue in degrees), matching the classic
    /// per-layer hue convention `hsl(min_bit * 30°, 0.8, 0.5)`.
    pub fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> Self {
        let h = hue.rem_euclid(360.0) / 60.0;
        let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
        let (r1, g1, b1) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = lightness - c / 2.0;
        Self::rgb(r1 + m, g1 + m, b1 + m)
    }
}

/// The authored appearance of a body.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct Appearance {
    /// Fill color of the extruded mesh.
    pub fill: Rgba,
    /// Outline color (alpha 0 hides the outline).
    #[serde(default = "default_border")]
    pub border: Rgba,
    /// Emissive strength: 0 = matte, higher values glow in `fill`'s hue.
    #[serde(default)]
    pub emissive: f32,
}

fn default_border() -> Rgba {
    Rgba {
        r: 0.13,
        g: 0.13,
        b: 0.15,
        a: 1.0,
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            fill: Rgba::rgb(0.6, 0.6, 0.65),
            border: default_border(),
            emissive: 0.0,
        }
    }
}
