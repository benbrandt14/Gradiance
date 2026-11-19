//! The scene look: one named [`Look`] decides every colour the *scene*
//! draws (body palette, borders, ground, backdrop, overlay ink) and the
//! lighting and toon settings it starts from (brief 4b-3).
//!
//! **One source.** This module is the only place a scene colour is written
//! as numbers; `tests/boundaries.rs` scans the scene crates and fails on a
//! colour literal anywhere else (the same rule `ui::fonts::glyph` keeps for
//! non-ASCII characters). The egui chrome is not scene colour: its tokens
//! are U-7's (`docs/unified-surface-plan.md` § 7.1).
//!
//! **Semantic palette, toned per look.** Bodies author a fill from
//! [`PALETTE`](crate::appearance::PALETTE), which is the default look's
//! palette. A palette slot means "red", not one exact RGB: the render layer
//! shows a palette-slot fill (and the default border, neutral fill and
//! ground) in the active look's tone through [`SceneTheme::display`]. So
//! switching looks recolours every palette-coloured body at once, a
//! custom colour stays exactly as authored, and the save file never
//! changes with the look.
//!
//! The colour math (relative luminance, contrast ratio, `OKLab` distance) is
//! here too, pure, so the consistency tests run without a renderer.

use crate::appearance::{Rgba, Swatch};
use crate::ramp::{ColorRamp, RampInterp};
use crate::settings::{KeyLightSettings, LightingSettings, RenderSettings, ScenerySettings};
use gradiance_units::Length;
use serde::{Deserialize, Serialize};

/// Studio's face relief ([`RenderSettings::relief`]): how far a front
/// face's normal tilts at the outline (1 = edge-on). Brief 4b-6 derives the
/// tilt from the distance to the outline; see [`STUDIO_BEVEL`].
pub const STUDIO_RELIEF: f32 = 1.0;

/// Studio's bevel ([`RenderSettings::bevel`]): the relief reaches this far
/// in from the outline, as a fraction of the shape's half-thickness. On
/// lavapipe (brief 4b-6) 0.45 left a circle a flat puck, 0.7 turned a box
/// into a pyramid; 0.6 rounds a circle most of the way and keeps a box a
/// slab with lit, chamfered edges.
pub const STUDIO_BEVEL: f32 = 0.6;

/// A named scene look. Scene content: it lives in
/// [`RenderSettings::look`], so it is saved with the scene and a change is
/// one undo step.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum Look {
    /// Clay studio (the default): warm light-grey backdrop, clay-toned
    /// pieces with dark outlines, a warm key and a cool fill light whose
    /// overlapping shadows show all four toon bands.
    #[default]
    Studio,
    /// Blueprint: navy backdrop, pale chalk pieces with ink outlines, one
    /// cool key light, three bands and a strong rim.
    Blueprint,
    /// Flat graphic: paper-white backdrop, bold saturated pieces with black
    /// outlines, two bands (lit or shadow) and no rim or glint.
    Poster,
}

impl Look {
    /// Every look, in menu order.
    pub const ALL: [Self; 3] = [Self::Studio, Self::Blueprint, Self::Poster];

    /// The stable lower-case name scripts and the remote surface use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.theme().name
    }

    /// The look called `name`, if any.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.name() == name)
    }

    /// The look's colours.
    #[must_use]
    pub const fn theme(self) -> &'static SceneTheme {
        match self {
            Self::Studio => &STUDIO,
            Self::Blueprint => &BLUEPRINT,
            Self::Poster => &POSTER,
        }
    }

    /// The render settings this look starts from.
    ///
    /// Only studio lights its faces with relief ([`STUDIO_RELIEF`]) and
    /// dithers its band edges: blueprint and poster are flat-colour styles,
    /// so their faces stay one tone and their bands stay in the shadows.
    /// No look has a glint: the matte default (brief 4b-6) puts a lit patch
    /// in the ramp instead.
    #[must_use]
    pub fn render(self) -> RenderSettings {
        let (rim_strength, white_point, relief, dither) = match self {
            Self::Studio => (0.2, STUDIO_WHITE_POINT, STUDIO_RELIEF, STUDIO_DITHER),
            Self::Blueprint => (0.35, 1.0, 0.0, 0.0),
            Self::Poster => (0.0, 1.0, 0.0, 0.0),
        };
        RenderSettings {
            look: self,
            toon_enabled: true,
            ramp: self.ramp(),
            dither,
            dither_scale: STUDIO_DITHER_SCALE,
            rim_strength,
            white_point,
            specular: 0.0,
            relief,
            bevel: STUDIO_BEVEL,
        }
    }

    /// The colour ramp this look starts from (brief 4b-6; tones by hard
    /// light, `ramp` module docs).
    ///
    /// - **Studio:** five bands, matching the lights (`lighting`): lit by
    ///   both lights the fill shows as authored, a face turned to the key
    ///   gets a warm cream patch, and the shadows shift hue toward a cool
    ///   mauve as they deepen (not only darker).
    /// - **Blueprint:** three cool bands at the old thirds.
    /// - **Poster:** two bands, lit or shadow.
    #[must_use]
    pub fn ramp(self) -> ColorRamp {
        use crate::ramp::neutral;
        let stops: Vec<(f32, Rgba)> = match self {
            Self::Studio => vec![
                (0.0, STUDIO_RAMP_DEEP),
                (0.3, STUDIO_RAMP_SHADOW),
                (0.5, STUDIO_RAMP_MID),
                (0.68, neutral()),
                (0.9, STUDIO_RAMP_LIT),
            ],
            Self::Blueprint => vec![
                (0.0, BLUEPRINT_RAMP_SHADOW),
                (1.0 / 3.0, BLUEPRINT_RAMP_MID),
                (2.0 / 3.0, neutral()),
            ],
            Self::Poster => vec![(0.0, POSTER_RAMP_SHADOW), (0.5, neutral())],
        };
        ColorRamp::new(RampInterp::Constant, &stops)
    }

    /// The lighting this look starts from.
    ///
    /// The light levels are measured against the toon white point (1.0), on
    /// a face toward the viewer: studio's ambient alone is about 0.2 of it,
    /// the fill adds 0.25 and the key 0.5, so lit by both a face lands in
    /// the top band, in the fill's shadow one band down, in the key's two,
    /// in both three — overlapping shadows read as a staircase of bands.
    /// Blueprint and poster put their one shadow at about 0.4 (one band
    /// down of three or two). Measured on lavapipe (brief 4b-3 Outcome).
    #[must_use]
    pub fn lighting(self) -> LightingSettings {
        let lights = match self {
            Self::Studio => vec![
                KeyLightSettings {
                    azimuth_deg: 125.0,
                    elevation_deg: 40.0,
                    color: STUDIO_KEY,
                    illuminance: 3_000.0,
                    shadows: true,
                },
                KeyLightSettings {
                    azimuth_deg: 30.0,
                    elevation_deg: 60.0,
                    color: FILL_LIGHT,
                    illuminance: 1_250.0,
                    shadows: true,
                },
            ],
            Self::Blueprint => vec![KeyLightSettings {
                azimuth_deg: 110.0,
                elevation_deg: 45.0,
                color: BLUEPRINT_KEY,
                illuminance: 3_000.0,
                shadows: true,
            }],
            Self::Poster => vec![KeyLightSettings {
                azimuth_deg: 135.0,
                elevation_deg: 45.0,
                color: WHITE_LIGHT,
                illuminance: 2_450.0,
                shadows: true,
            }],
        };
        let ambient = match self {
            Self::Studio => 330.0,
            Self::Blueprint => 670.0,
            Self::Poster => 750.0,
        };
        LightingSettings {
            lights,
            ambient,
            ssao: false,
            contact_shadows: true,
            shadow_map_size: 4096,
            shadow_distance: Length(80.0),
        }
    }

    /// The scenery (backdrop) this look starts from.
    #[must_use]
    pub fn scenery(self) -> ScenerySettings {
        ScenerySettings {
            back_offset: Length(0.2),
            back_color: self.theme().backdrop,
            back_visible: true,
            ground_visible: true,
            perspective_deg: 0.0,
        }
    }
}

/// The theme `settings` select; the default look's when there are none (a
/// headless app without the render settings).
#[must_use]
pub fn active(settings: Option<&RenderSettings>) -> &'static SceneTheme {
    settings.map_or_else(Look::default, |s| s.look).theme()
}

/// Overlay ink: the colours editor gizmos draw with, per look. Each role
/// is one token, so a tool never picks its own colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    /// The one accent: selection outline, selected joint, live span.
    pub accent: Rgba,
    /// Snap glyphs, anchor dots, the polygon close marker.
    pub snap: Rgba,
    /// Constraint and alignment guides (drawn with reduced alpha).
    pub guide: Rgba,
    /// Creation-tool rubber bands.
    pub preview: Rgba,
    /// The alternate gesture: array mode, springs.
    pub alternate: Rgba,
    /// Destructive strokes: the cut line, a world pin.
    pub danger: Rgba,
    /// The selection box and scale handles.
    pub marquee: Rgba,
    /// The rotate handle.
    pub rotate: Rgba,
    /// Joint glyph face.
    pub joint: Rgba,
    /// Joint glyph outline, and the rod preview.
    pub joint_outline: Rgba,
    /// Inactive marks: a broken joint, a circle's radius line.
    pub muted: Rgba,
    /// The reference grid's line colour (minor, major and axis lines
    /// differ by alpha only).
    pub grid: Rgba,
}

/// An overlay ink role: what a stroke *means*, resolved to a colour by the
/// active look ([`Ink::get`]). Tools draw with roles, never colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// [`Ink::accent`].
    Accent,
    /// [`Ink::snap`].
    Snap,
    /// [`Ink::guide`].
    Guide,
    /// [`Ink::preview`].
    Preview,
    /// [`Ink::alternate`].
    Alternate,
    /// [`Ink::danger`].
    Danger,
    /// [`Ink::marquee`].
    Marquee,
    /// [`Ink::rotate`].
    Rotate,
    /// [`Ink::joint`].
    Joint,
    /// [`Ink::joint_outline`].
    JointOutline,
    /// [`Ink::muted`].
    Muted,
    /// [`Ink::grid`].
    Grid,
}

impl Role {
    /// This role at reduced opacity.
    #[must_use]
    pub const fn alpha(self, alpha: f32) -> Stroke {
        Stroke { role: self, alpha }
    }
}

/// A [`Role`] at an opacity: one stroke of overlay ink.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// What the stroke means.
    pub role: Role,
    /// Its opacity in `[0, 1]`.
    pub alpha: f32,
}

impl From<Role> for Stroke {
    fn from(role: Role) -> Self {
        role.alpha(1.0)
    }
}

impl Ink {
    /// The colour of `role`.
    #[must_use]
    pub const fn get(&self, role: Role) -> Rgba {
        match role {
            Role::Accent => self.accent,
            Role::Snap => self.snap,
            Role::Guide => self.guide,
            Role::Preview => self.preview,
            Role::Alternate => self.alternate,
            Role::Danger => self.danger,
            Role::Marquee => self.marquee,
            Role::Rotate => self.rotate,
            Role::Joint => self.joint,
            Role::JointOutline => self.joint_outline,
            Role::Muted => self.muted,
            Role::Grid => self.grid,
        }
    }

    /// The colour of `stroke`: its role's colour at its opacity.
    #[must_use]
    pub fn stroke(&self, stroke: impl Into<Stroke>) -> Rgba {
        let stroke = stroke.into();
        self.get(stroke.role).with_alpha(stroke.alpha)
    }
}

/// Every colour of one [`Look`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneTheme {
    /// The look's stable name.
    pub name: &'static str,
    /// The menu label.
    pub label: &'static str,
    /// The body palette, slot for slot with [`PALETTE`](crate::appearance::PALETTE).
    pub palette: [Swatch; 10],
    /// A body's fill before it is given a colour.
    pub neutral: Rgba,
    /// A new ground's fill.
    pub ground: Rgba,
    /// A new rod's fill (a dark bar in a light look, a pale one in a dark).
    pub rod: Rgba,
    /// Body outlines.
    pub border: Rgba,
    /// The back plane, and the clear colour the horizon fades to.
    pub backdrop: Rgba,
    /// Overlay ink.
    pub ink: Ink,
}

impl SceneTheme {
    /// How an authored fill shows under this look: a [`STUDIO`] palette
    /// slot, the neutral fill, the ground, the rod or the default border becomes
    /// this look's tone (alpha kept); any other colour is shown as authored.
    #[must_use]
    pub fn display(&self, authored: Rgba) -> Rgba {
        let canonical = &STUDIO;
        let toned = if let Some(slot) = crate::appearance::palette_slot(authored) {
            self.palette[slot].color
        } else if same_rgb(authored, canonical.neutral) {
            self.neutral
        } else if same_rgb(authored, canonical.ground) {
            self.ground
        } else if same_rgb(authored, canonical.rod) {
            self.rod
        } else if same_rgb(authored, canonical.border) {
            self.border
        } else {
            return authored;
        };
        Rgba {
            a: authored.a,
            ..toned
        }
    }
}

/// RGB equality within a save-file round trip.
fn same_rgb(a: Rgba, b: Rgba) -> bool {
    let near = |x: f32, y: f32| (x - y).abs() < 1e-5;
    near(a.r, b.r) && near(a.g, b.g) && near(a.b, b.b)
}

const fn swatch(name: &'static str, r: f32, g: f32, b: f32) -> Swatch {
    Swatch {
        name,
        color: Rgba::rgb(r, g, b),
    }
}

/// White light (an untinted lamp).
pub const WHITE_LIGHT: Rgba = Rgba::rgb(1.0, 1.0, 1.0);
/// The studio key: a warm lamp.
/// Studio's white point: a face lit by both lights lands at about 0.76 of
/// the ramp (the authored-fill band), so a face turned toward the key can
/// rise into the lit patch above it (brief 4b-6, measured on lavapipe).
pub const STUDIO_WHITE_POINT: f32 = 1.25;
/// Studio's band-edge dither width, in lighting levels.
pub const STUDIO_DITHER: f32 = 0.025;
/// Screen pixels per dither cell (every look).
pub const STUDIO_DITHER_SCALE: f32 = 2.0;
/// Studio ramp: both shadows, a cool mauve-brown (hue-shifted, not only
/// darker).
const STUDIO_RAMP_DEEP: Rgba = Rgba::rgb(0.28, 0.25, 0.32);
/// Studio ramp: one light's shadow.
const STUDIO_RAMP_SHADOW: Rgba = Rgba::rgb(0.34, 0.31, 0.37);
/// Studio ramp: the fill's shadow, nearly the fill.
const STUDIO_RAMP_MID: Rgba = Rgba::rgb(0.42, 0.4, 0.43);
/// Studio ramp: the lit patch, a warm cream in the key light's hue.
const STUDIO_RAMP_LIT: Rgba = Rgba::rgb(0.72, 0.7, 0.56);
/// Blueprint ramp: the shadow, toward navy.
const BLUEPRINT_RAMP_SHADOW: Rgba = Rgba::rgb(0.27, 0.3, 0.38);
/// Blueprint ramp: the middle band.
const BLUEPRINT_RAMP_MID: Rgba = Rgba::rgb(0.4, 0.42, 0.46);
/// Poster ramp: the shadow (about half the light, as its old two bands).
const POSTER_RAMP_SHADOW: Rgba = Rgba::rgb(0.36, 0.36, 0.36);
const STUDIO_KEY: Rgba = Rgba::rgb(1.0, 0.95, 0.88);
/// The studio fill: a cool sky. Also the tint of a light added in the
/// Lighting tab.
pub const FILL_LIGHT: Rgba = Rgba::rgb(0.84, 0.9, 1.0);
/// The blueprint key: a cool lamp.
const BLUEPRINT_KEY: Rgba = Rgba::rgb(0.9, 0.95, 1.0);

// The values below were designed in OKLCH (one lightness and chroma per
// look, ten hues; chroma reduced where the hue leaves the sRGB gamut) and
// written out as sRGB. The tests at the bottom of this file hold them to
// the contrast and distinctness floors.

/// Clay studio: OKLCH L 0.585, C ≤ 0.13 fills on an L 0.925 warm grey.
pub const STUDIO: SceneTheme = SceneTheme {
    name: "studio",
    label: "Clay studio",
    palette: [
        swatch("red", 0.740, 0.348, 0.327),
        swatch("orange", 0.710, 0.390, 0.127),
        swatch("amber", 0.610, 0.459, 0.029),
        swatch("lime", 0.417, 0.535, 0.141),
        swatch("green", 0.211, 0.567, 0.315),
        swatch("teal", 0.024, 0.558, 0.537),
        swatch("sky", 0.011, 0.531, 0.702),
        swatch("blue", 0.335, 0.471, 0.787),
        swatch("violet", 0.534, 0.407, 0.740),
        swatch("pink", 0.692, 0.349, 0.554),
    ],
    neutral: Rgba::rgb(0.500, 0.483, 0.455),
    ground: Rgba::rgb(0.413, 0.384, 0.346),
    rod: Rgba::rgb(0.08, 0.08, 0.09),
    border: Rgba::rgb(0.163, 0.135, 0.113),
    backdrop: Rgba::rgb(0.919, 0.900, 0.869),
    ink: Ink {
        accent: Rgba::rgb(0.810, 0.337, 0.017),
        snap: Rgba::rgb(0.728, 0.009, 0.478),
        guide: Rgba::rgb(0.078, 0.406, 0.760),
        preview: Rgba::rgb(0.034, 0.461, 0.538),
        alternate: Rgba::rgb(0.020, 0.536, 0.243),
        danger: Rgba::rgb(0.760, 0.089, 0.145),
        marquee: Rgba::rgb(0.089, 0.385, 0.713),
        rotate: Rgba::rgb(0.432, 0.243, 0.679),
        joint: Rgba::rgb(0.968, 0.960, 0.947),
        joint_outline: Rgba::rgb(0.150, 0.183, 0.218),
        muted: Rgba::rgb(0.346, 0.332, 0.310),
        grid: Rgba::rgb(0.412, 0.450, 0.491),
    },
};

/// Blueprint: OKLCH L 0.84, C ≤ 0.105 fills on an L 0.30 navy.
pub const BLUEPRINT: SceneTheme = SceneTheme {
    name: "blueprint",
    label: "Blueprint",
    palette: [
        swatch("red", 0.998, 0.709, 0.682),
        swatch("orange", 0.997, 0.726, 0.543),
        swatch("amber", 0.918, 0.774, 0.470),
        swatch("lime", 0.733, 0.843, 0.542),
        swatch("green", 0.596, 0.871, 0.652),
        swatch("teal", 0.432, 0.881, 0.853),
        swatch("sky", 0.513, 0.843, 0.999),
        swatch("blue", 0.699, 0.794, 0.995),
        swatch("violet", 0.829, 0.742, 0.999),
        swatch("pink", 0.990, 0.686, 0.856),
    ],
    neutral: Rgba::rgb(0.761, 0.798, 0.846),
    ground: Rgba::rgb(0.018, 0.104, 0.210),
    rod: Rgba::rgb(0.902, 0.929, 0.965),
    border: Rgba::rgb(0.044, 0.133, 0.241),
    backdrop: Rgba::rgb(0.074, 0.181, 0.314),
    ink: Ink {
        accent: Rgba::rgb(0.997, 0.700, 0.319),
        snap: Rgba::rgb(0.938, 0.892, 0.270),
        guide: Rgba::rgb(0.347, 0.819, 0.897),
        preview: Rgba::rgb(0.477, 0.901, 0.899),
        alternate: Rgba::rgb(0.424, 0.913, 0.554),
        danger: Rgba::rgb(0.981, 0.413, 0.360),
        marquee: Rgba::rgb(0.526, 0.804, 0.998),
        rotate: Rgba::rgb(0.776, 0.600, 0.973),
        joint: Rgba::rgb(0.942, 0.964, 0.987),
        joint_outline: Rgba::rgb(0.044, 0.133, 0.241),
        muted: Rgba::rgb(0.480, 0.531, 0.598),
        grid: Rgba::rgb(0.528, 0.784, 0.908),
    },
};

/// Flat graphic: OKLCH L 0.615, C ≤ 0.19 fills on an L 0.965 paper white.
pub const POSTER: SceneTheme = SceneTheme {
    name: "poster",
    label: "Flat graphic",
    palette: [
        swatch("red", 0.878, 0.278, 0.274),
        swatch("orange", 0.783, 0.403, 0.004),
        swatch("amber", 0.655, 0.491, 0.001),
        swatch("lime", 0.438, 0.578, 0.008),
        swatch("green", 0.023, 0.624, 0.287),
        swatch("teal", 0.005, 0.598, 0.575),
        swatch("sky", 0.012, 0.569, 0.751),
        swatch("blue", 0.294, 0.484, 0.961),
        swatch("violet", 0.596, 0.385, 0.893),
        swatch("pink", 0.811, 0.288, 0.621),
    ],
    neutral: Rgba::rgb(0.520, 0.520, 0.520),
    ground: Rgba::rgb(0.122, 0.122, 0.122),
    rod: Rgba::rgb(0.08, 0.08, 0.08),
    border: Rgba::rgb(0.069, 0.069, 0.069),
    backdrop: Rgba::rgb(0.970, 0.954, 0.911),
    ink: Ink {
        accent: Rgba::rgb(0.873, 0.143, 0.080),
        snap: Rgba::rgb(0.668, 0.010, 0.535),
        guide: Rgba::rgb(0.160, 0.333, 0.827),
        preview: Rgba::rgb(0.024, 0.426, 0.565),
        alternate: Rgba::rgb(0.020, 0.536, 0.243),
        danger: Rgba::rgb(0.734, 0.007, 0.115),
        marquee: Rgba::rgb(0.164, 0.322, 0.757),
        rotate: Rgba::rgb(0.451, 0.155, 0.684),
        joint: Rgba::rgb(1.000, 1.000, 1.000),
        joint_outline: Rgba::rgb(0.069, 0.069, 0.069),
        muted: Rgba::rgb(0.334, 0.334, 0.334),
        grid: Rgba::rgb(0.389, 0.389, 0.389),
    },
};

/// Debug-overlay and solver-ghost colours: diagnostic, not part of any
/// look (they must stay recognisable while the look changes). Named by
/// what they mark.
pub mod debug {
    use crate::appearance::Rgba;

    /// Collider outlines (the physics view of shapes).
    pub const COLLIDER: Rgba = Rgba::rgb(0.0, 1.0, 0.498);
    /// World-space AABBs.
    pub const AABB: Rgba = Rgba::rgb(0.741, 0.718, 0.420);
    /// Body origin crosshairs.
    pub const ORIGIN: Rgba = Rgba::rgb(1.0, 1.0, 1.0);
    /// Shape centroids.
    pub const CENTROID: Rgba = Rgba::rgb(1.0, 0.412, 0.706);
    /// A sleeping body's ring.
    pub const SLEEPING: Rgba = Rgba::rgb(0.502, 0.502, 0.502);
    /// Linear velocity arrows.
    pub const VELOCITY: Rgba = Rgba::rgb(1.0, 1.0, 0.0);
    /// Angular velocity arcs.
    pub const SPIN: Rgba = Rgba::rgb(1.0, 0.647, 0.0);
    /// Hinge anchors.
    pub const HINGE: Rgba = Rgba::rgb(1.0, 0.647, 0.0);
    /// Slider anchors.
    pub const SLIDER: Rgba = Rgba::rgb(0.0, 0.749, 1.0);
    /// Spring anchors.
    pub const SPRING: Rgba = Rgba::rgb(0.0, 1.0, 0.498);
    /// Contact points and normals.
    pub const CONTACT: Rgba = Rgba::rgb(1.0, 0.0, 1.0);
    /// Substep traces.
    pub const SUBSTEP: Rgba = Rgba::rgb(0.729, 0.333, 0.827);
    /// Force-field arrows, weak end.
    pub const FIELD_COLD: Rgba = Rgba::rgb(0.275, 0.510, 0.706);
    /// Force-field arrows, strong end.
    pub const FIELD_HOT: Rgba = Rgba::rgb(1.0, 0.271, 0.0);
    /// An attracting field source.
    pub const FIELD_ATTRACT: Rgba = Rgba::rgb(0.392, 0.584, 0.929);
    /// A repelling field source.
    pub const FIELD_REPEL: Rgba = Rgba::rgb(0.804, 0.361, 0.361);
    /// The pack ghost: a candidate pose.
    pub const PACK_GHOST: Rgba = Rgba::rgb(0.439, 0.502, 0.565);
    /// The pack ghost: the run finished.
    pub const PACK_DONE: Rgba = Rgba::rgb(0.0, 1.0, 0.498);
    /// The pack ghost: still searching.
    pub const PACK_RUNNING: Rgba = Rgba::rgb(0.498, 1.0, 0.831);
    /// The pack ghost: a pinned obstacle.
    pub const PACK_PINNED: Rgba = Rgba::rgb(1.0, 0.647, 0.0);

    /// A collision layer's outline colour: the classic per-layer hue
    /// (`hsl(hue, 0.75, 0.55)`), matching the depth panel's swatches.
    #[must_use]
    pub fn layer(hue: f32) -> Rgba {
        Rgba::from_hsl(hue, 0.75, 0.55)
    }
}

// --- colour math ---------------------------------------------------------

/// sRGB transfer function, decoded: an sRGB channel to linear light.
#[must_use]
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of an sRGB colour (alpha ignored).
#[must_use]
pub fn relative_luminance(c: Rgba) -> f32 {
    0.2126 * srgb_to_linear(c.r) + 0.7152 * srgb_to_linear(c.g) + 0.0722 * srgb_to_linear(c.b)
}

/// WCAG contrast ratio between two colours, in `[1, 21]`; order does not
/// matter. 3:1 is the WCAG floor for graphical objects.
#[must_use]
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// An sRGB colour in `OKLab` `(L, a, b)` (Björn Ottosson's matrices).
#[must_use]
pub fn oklab(c: Rgba) -> [f32; 3] {
    let (r, g, b) = (
        srgb_to_linear(c.r),
        srgb_to_linear(c.g),
        srgb_to_linear(c.b),
    );
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// Perceptual distance between two colours: Euclidean distance in `OKLab`.
/// About 0.02 is a just-noticeable difference.
#[must_use]
pub fn delta_e(a: Rgba, b: Rgba) -> f32 {
    let (p, q) = (oklab(a), oklab(b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

/// Consistency floors every look is held to (the brief's testable
/// consistency). Public so a future look editor can show the same checks.
pub mod floors {
    /// A body's fill against the backdrop: the WCAG 3:1 floor for
    /// graphical objects.
    pub const FILL_VS_BACKDROP: f32 = 3.0;
    /// A body's outline against its fill: 3:1, so two touching bodies of
    /// one colour still read as two.
    pub const BORDER_VS_FILL: f32 = 3.0;
    /// Overlay ink (every role but the joint face) against the backdrop.
    pub const INK_VS_BACKDROP: f32 = 3.0;
    /// The joint glyph's face against its outline.
    pub const JOINT_FACE_VS_OUTLINE: f32 = 3.0;
    /// The ground against the backdrop. A surface tone, not an object: the
    /// ground's trace line and the shadows on it carry its edge, so the
    /// floor is lower.
    pub const GROUND_VS_BACKDROP: f32 = 1.2;
    /// Pairwise `OKLab` distance between palette entries: more than twice a
    /// just-noticeable difference, so "every red body" is unambiguous.
    pub const PALETTE_DELTA_E: f32 = 0.045;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::PALETTE;

    fn report(failures: &[String]) {
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn the_canonical_palette_is_the_default_looks() {
        assert_eq!(Look::default(), Look::Studio);
        for (canonical, studio) in PALETTE.iter().zip(STUDIO.palette) {
            assert_eq!(*canonical, studio);
        }
        for look in Look::ALL {
            for (a, b) in PALETTE.iter().zip(look.theme().palette) {
                assert_eq!(a.name, b.name, "slot names agree across looks");
            }
            assert_eq!(Look::named(look.name()), Some(look));
        }
        assert_eq!(Look::named("nope"), None);
    }

    #[test]
    fn every_fill_stands_off_the_backdrop_and_its_border() {
        let mut failures = Vec::new();
        for look in Look::ALL {
            let t = look.theme();
            let fills = t
                .palette
                .iter()
                .map(|s| (s.name, s.color))
                .chain([("neutral", t.neutral)]);
            for (name, fill) in fills {
                let bg = contrast_ratio(fill, t.backdrop);
                if bg < floors::FILL_VS_BACKDROP {
                    failures.push(format!("{}: {name} vs backdrop {bg:.2}", t.name));
                }
                let border = contrast_ratio(fill, t.border);
                if border < floors::BORDER_VS_FILL {
                    failures.push(format!("{}: border vs {name} {border:.2}", t.name));
                }
            }
            let rod = contrast_ratio(t.rod, t.backdrop);
            if rod < floors::FILL_VS_BACKDROP {
                failures.push(format!("{}: rod vs backdrop {rod:.2}", t.name));
            }
            let ground = contrast_ratio(t.ground, t.backdrop);
            if ground < floors::GROUND_VS_BACKDROP {
                failures.push(format!("{}: ground vs backdrop {ground:.2}", t.name));
            }
        }
        report(&failures);
    }

    #[test]
    fn overlay_ink_reads_on_the_backdrop() {
        let mut failures = Vec::new();
        for look in Look::ALL {
            let t = look.theme();
            let i = t.ink;
            let roles = [
                ("accent", i.accent),
                ("snap", i.snap),
                ("guide", i.guide),
                ("preview", i.preview),
                ("alternate", i.alternate),
                ("danger", i.danger),
                ("marquee", i.marquee),
                ("rotate", i.rotate),
                ("muted", i.muted),
                ("grid", i.grid),
            ];
            for (name, ink) in roles {
                let c = contrast_ratio(ink, t.backdrop);
                if c < floors::INK_VS_BACKDROP {
                    failures.push(format!("{}: {name} ink vs backdrop {c:.2}", t.name));
                }
            }
            let joint = contrast_ratio(i.joint, i.joint_outline);
            if joint < floors::JOINT_FACE_VS_OUTLINE {
                failures.push(format!("{}: joint face vs outline {joint:.2}", t.name));
            }
        }
        report(&failures);
    }

    #[test]
    fn palette_entries_are_pairwise_distinct() {
        let mut failures = Vec::new();
        for look in Look::ALL {
            let p = look.theme().palette;
            for (i, a) in p.iter().enumerate() {
                for b in &p[i + 1..] {
                    let d = delta_e(a.color, b.color);
                    if d < floors::PALETTE_DELTA_E {
                        failures.push(format!(
                            "{}: {} vs {} dE {d:.3}",
                            look.name(),
                            a.name,
                            b.name
                        ));
                    }
                }
            }
        }
        report(&failures);
    }

    #[test]
    fn display_tones_canonical_colours_and_keeps_custom_ones() {
        let custom = Rgba::rgb(0.123, 0.456, 0.789);
        for look in Look::ALL {
            let t = look.theme();
            assert_eq!(t.display(custom), custom, "custom colours are untouched");
            for (slot, swatch) in PALETTE.iter().enumerate() {
                let authored = Rgba {
                    a: 0.5,
                    ..swatch.color
                };
                let shown = t.display(authored);
                assert_eq!(
                    shown,
                    Rgba {
                        a: 0.5,
                        ..t.palette[slot].color
                    }
                );
            }
            assert_eq!(t.display(STUDIO.border), t.border);
            assert_eq!(t.display(STUDIO.ground), t.ground);
            assert_eq!(t.display(STUDIO.neutral), t.neutral);
            assert_eq!(t.display(STUDIO.rod), t.rod);
        }
        // The default look shows everything as authored.
        for swatch in PALETTE {
            assert_eq!(STUDIO.display(swatch.color), swatch.color);
        }
    }

    #[test]
    fn the_colour_math_matches_known_values() {
        let white = Rgba::rgb(1.0, 1.0, 1.0);
        let black = Rgba::rgb(0.0, 0.0, 0.0);
        assert!((contrast_ratio(white, black) - 21.0).abs() < 1e-3);
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-3);
        assert!((relative_luminance(white) - 1.0).abs() < 1e-4);
        // OKLab of white is (1, 0, 0); of sRGB red about (0.628, 0.225, 0.126).
        let w = oklab(white);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1].abs() < 1e-3 && w[2].abs() < 1e-3);
        let r = oklab(Rgba::rgb(1.0, 0.0, 0.0));
        assert!((r[0] - 0.628).abs() < 2e-3, "{r:?}");
        assert!((r[1] - 0.225).abs() < 2e-3, "{r:?}");
        assert!((r[2] - 0.126).abs() < 2e-3, "{r:?}");
        assert!(delta_e(white, white) < 1e-6);
    }

    #[test]
    fn presets_carry_their_look() {
        for look in Look::ALL {
            assert_eq!(look.render().look, look);
            assert_eq!(look.scenery().back_color, look.theme().backdrop);
            assert_ne!(look.lighting().lights.len(), 0);
            assert!(!look.lighting().ssao, "toon, not SSAO, is the default");
        }
    }
}
