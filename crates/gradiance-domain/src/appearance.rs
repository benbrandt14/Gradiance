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
#[derive(
    Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
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

impl Appearance {
    /// Default appearance for a newly authored body: a random pleasant hue
    /// derived from its stable id (Algodoo behavior — every new body gets
    /// its own color; deterministic per id, so undo/redo repaint the same).
    ///
    /// The one rule for every authoring surface — the drawing tools and the
    /// scripting verbs both call it, so a scripted scene and a hand-built
    /// one are indistinguishable.
    ///
    /// The fill is a [`PALETTE`] entry (the themed default palette), not an
    /// arbitrary hue, so a scene built by hand or by script reads as one set.
    #[must_use]
    pub fn for_id(id: gradiance_core::ids::StableId) -> Self {
        Self {
            fill: PALETTE[palette_index(id)].color,
            ..Default::default()
        }
    }
}

/// One named entry of the themed default palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swatch {
    /// Stable lower-case name (an API for scripts and a future shell).
    pub name: &'static str,
    /// The fill color.
    pub color: Rgba,
}

/// The themed default palette: ten hues at one saturation and lightness
/// (`hsl(h, 0.65, 0.55)`), so any mix of them sits together.
///
/// The one table every surface reads — [`Appearance::for_id`] for new
/// bodies, the inspector's swatch row (the quantized picker), and
/// [`random_fills`]. Data, so a replacement UI shell reads the same colors.
pub const PALETTE: [Swatch; 10] = [
    swatch("red", 0.843, 0.257, 0.257),
    swatch("orange", 0.843, 0.501, 0.257),
    swatch("amber", 0.843, 0.696, 0.257),
    swatch("lime", 0.550, 0.843, 0.257),
    swatch("green", 0.257, 0.843, 0.501),
    swatch("teal", 0.257, 0.843, 0.794),
    swatch("sky", 0.257, 0.647, 0.843),
    swatch("blue", 0.257, 0.404, 0.843),
    swatch("violet", 0.501, 0.257, 0.843),
    swatch("pink", 0.843, 0.257, 0.696),
];

const fn swatch(name: &'static str, r: f32, g: f32, b: f32) -> Swatch {
    Swatch {
        name,
        color: Rgba::rgb(r, g, b),
    }
}

/// The [`PALETTE`] slot a stable id lands on (deterministic per id, so
/// undo/redo and reload repaint the same).
#[must_use]
pub fn palette_index(id: gradiance_core::ids::StableId) -> usize {
    (id.0.as_u128() % PALETTE.len() as u128) as usize
}

/// Palette step between successive colors handed out by [`random_fills`]:
/// coprime with the palette length, so the first `PALETTE.len()` keys all
/// differ and neighbours are never adjacent hues.
const SPREAD_STRIDE: usize = 3;

/// "Random colors" over a selection: new appearances drawn from the
/// [`PALETTE`].
///
/// `members` is `(id, outermost group, current appearance)` per body. With
/// `grouped`, bodies sharing an outermost [`SelectionGroup`](crate::group::SelectionGroup)
/// share one color (an ungrouped body is its own group); otherwise every
/// body gets its own. Distinct keys get distinct colors, in order of first
/// appearance, starting from the first body's own palette slot. Only the
/// fill's RGB changes — alpha, border and emissive are kept.
#[must_use]
pub fn random_fills(
    members: &[(gradiance_core::ids::StableId, Option<u32>, Appearance)],
    grouped: bool,
) -> Vec<(gradiance_core::ids::StableId, Appearance)> {
    #[derive(PartialEq, Clone, Copy)]
    enum Key {
        Body(gradiance_core::ids::StableId),
        Group(u32),
    }
    let Some(seed) = members.first().map(|(id, ..)| palette_index(*id)) else {
        return Vec::new();
    };
    let mut keys: Vec<Key> = Vec::new();
    members
        .iter()
        .map(|(id, group, old)| {
            let key = match (grouped, group) {
                (true, Some(g)) => Key::Group(*g),
                _ => Key::Body(*id),
            };
            let slot = keys.iter().position(|k| *k == key).unwrap_or_else(|| {
                keys.push(key);
                keys.len() - 1
            });
            let color = PALETTE[(seed + slot * SPREAD_STRIDE) % PALETTE.len()].color;
            let fill = Rgba {
                a: old.fill.a,
                ..color
            };
            (*id, Appearance { fill, ..*old })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    // float_cmp: palette colors are copied, never computed.
    #![allow(clippy::float_cmp)]
    use super::*;
    use gradiance_core::ids::StableId;

    #[test]
    fn for_id_lands_on_the_palette() {
        for _ in 0..200 {
            let id = StableId::new();
            let fill = Appearance::for_id(id).fill;
            assert!(
                PALETTE.iter().any(|s| s.color == fill),
                "{fill:?} is not a palette color"
            );
            assert_eq!(fill, Appearance::for_id(id).fill, "deterministic per id");
        }
    }

    #[test]
    fn palette_names_are_unique() {
        for (i, a) in PALETTE.iter().enumerate() {
            assert!(PALETTE[i + 1..].iter().all(|b| b.name != a.name));
            assert!(PALETTE[i + 1..].iter().all(|b| b.color != a.color));
        }
    }

    fn member(group: Option<u32>) -> (StableId, Option<u32>, Appearance) {
        let mut old = Appearance::default();
        old.fill.a = 0.5;
        old.emissive = 0.3;
        (StableId::new(), group, old)
    }

    #[test]
    fn grouped_random_fills_share_one_color_per_group() {
        let members = vec![
            member(Some(7)),
            member(None),
            member(Some(7)),
            member(Some(9)),
            member(Some(9)),
            member(None),
        ];
        let out = random_fills(&members, true);
        assert_eq!(out.len(), members.len());
        let rgb = |i: usize| {
            let f = out[i].1.fill;
            (f.r, f.g, f.b)
        };
        // Group 7 = bodies 0 and 2; group 9 = bodies 3 and 4.
        assert_eq!(rgb(0), rgb(2));
        assert_eq!(rgb(3), rgb(4));
        // Four keys (g7, body 1, g9, body 5) — four different colors.
        let distinct = [rgb(0), rgb(1), rgb(3), rgb(5)];
        for (i, a) in distinct.iter().enumerate() {
            assert!(distinct[i + 1..].iter().all(|b| b != a));
        }
        // Everything but the fill RGB is kept, and ids pass through in order.
        for ((id, _, old), (out_id, new)) in members.iter().zip(&out) {
            assert_eq!(id, out_id);
            assert_eq!(new.fill.a, old.fill.a);
            assert_eq!(new.emissive, old.emissive);
            assert_eq!(new.border, old.border);
            assert!(PALETTE.iter().any(|s| s.color.r == new.fill.r
                && s.color.g == new.fill.g
                && s.color.b == new.fill.b));
        }
    }

    #[test]
    fn per_body_random_fills_ignore_groups() {
        let members = vec![member(Some(1)), member(Some(1)), member(Some(1))];
        let out = random_fills(&members, false);
        assert_ne!(out[0].1.fill, out[1].1.fill);
        assert_ne!(out[1].1.fill, out[2].1.fill);
        assert_ne!(out[0].1.fill, out[2].1.fill);
        assert_eq!(random_fills(&[], true), [] as [(StableId, Appearance); 0]);
    }
}
