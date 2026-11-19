//! Authored editor settings: grid and snapping configuration.
//!
//! These are scene-level settings (Algodoo persists grid setup with the
//! scene), so they live in `domain` and serialize into the save file's
//! environment section.

use bevy::math::Vec2;
use bevy::prelude::Resource;
use gradiance_units::{Acceleration2, Angle, Density, Displacement, Frequency, Length, Velocity2};
use serde::{Deserialize, Serialize};

/// The family of grid geometries.
///
/// Extensible: adding a variant means implementing its snap math in
/// `geometry::snapping` and its drawing in `render::grid`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub enum GridSystem {
    /// Standard square grid.
    Cartesian,
    /// Two line families at ±30° — isometric drafting.
    Isometric,
    /// Concentric rings and angular spokes around the grid origin.
    Polar {
        /// Number of angular divisions (spokes) per full turn.
        angular_divisions: u32,
    },
}

/// Grid configuration (visible reference + snap target).
///
/// The grid has its own origin and rotation — a movable "user coordinate
/// system" in CAD terms — so sketching against a tilted structure works.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct GridSettings {
    /// Draw the grid.
    pub visible: bool,
    /// Use the grid as a snap source.
    pub snap_enabled: bool,
    /// Grid geometry family.
    pub system: GridSystem,
    /// Base cell size (display adapts by powers of two).
    pub spacing: Length,
    /// Grid origin in world space.
    pub origin: Displacement,
    /// Grid rotation.
    pub rotation: Angle,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            visible: true,
            snap_enabled: false,
            system: GridSystem::Cartesian,
            spacing: Length(1.0),
            origin: Displacement(Vec2::ZERO),
            rotation: Angle(0.0),
        }
    }
}

/// Which object features act as snap sources (CAD "object snaps").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct SnapSources {
    /// Shape outline vertices.
    pub vertices: bool,
    /// Midpoints of outline edges.
    pub midpoints: bool,
    /// Body centers.
    pub centers: bool,
    /// Closest point anywhere along an edge.
    pub edges: bool,
}

impl Default for SnapSources {
    fn default() -> Self {
        Self {
            vertices: true,
            midpoints: true,
            centers: true,
            edges: true,
        }
    }
}

/// Snapping configuration.
///
/// Object snaps always take priority over the grid (CAD convention);
/// among object snaps, the nearest candidate wins.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct SnapConfig {
    /// Master toggle for object snapping.
    pub objects_enabled: bool,
    /// Screen-space capture radius in logical pixels.
    pub max_screen_distance: f32,
    /// Enabled object snap sources.
    pub sources: SnapSources,
    /// Rotation quantization step in degrees (applied while the quantize
    /// modifier is held, and by future gesture behaviors).
    pub rotation_step_deg: f32,
    /// While moving a selection, lightly snap its edges collinear with
    /// nearby bodies' edges along the grid axes (needs `objects_enabled`).
    #[serde(default = "default_true")]
    pub align_edges: bool,
    /// While moving a selection, lightly snap its centre onto nearby
    /// bodies' centre lines along the grid axes (needs `objects_enabled`).
    #[serde(default)]
    pub align_centers: bool,
}

/// Serde default for fields that were added defaulting on.
fn default_true() -> bool {
    true
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self {
            objects_enabled: true,
            max_screen_distance: 12.0,
            sources: SnapSources::default(),
            rotation_step_deg: 15.0,
            align_edges: true,
            align_centers: false,
        }
    }
}

/// Camera feel (the "Camera" section of the Grid & Snap settings tab).
///
/// **Workstation config**, like [`GridSettings`]: it describes the person
/// driving the editor, not the scene, so it is saved with the scene but never
/// enters undo (`EnvironmentRecord::scene_content_eq` leaves it out). The
/// camera reads it through [`sanitized`](Self::sanitized), so a value typed
/// out of range cannot freeze or invert the view.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
#[serde(default)]
pub struct CameraSettings {
    /// Zoom multiplier per scroll notch, in `(0, 1)`: closer to 1 is
    /// gentler, smaller zooms faster.
    pub zoom_step: f32,
    /// Keyboard (arrow-key) pan speed in screen pixels per second, so it
    /// feels the same at every zoom.
    pub pan_speed: f32,
    /// Middle-drag orbit sensitivity in degrees per screen pixel.
    pub orbit_speed_deg: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            zoom_step: 0.94,
            pan_speed: 600.0,
            orbit_speed_deg: 0.3,
        }
    }
}

impl CameraSettings {
    /// Smallest / largest zoom step the camera will use.
    pub const ZOOM_STEP_RANGE: (f32, f32) = (0.5, 0.995);

    /// The settings clamped to values the camera can use: a zoom step
    /// inside [`ZOOM_STEP_RANGE`](Self::ZOOM_STEP_RANGE), non-negative
    /// speeds, and the default for anything non-finite.
    #[must_use]
    pub fn sanitized(&self) -> Self {
        let d = Self::default();
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let (lo, hi) = Self::ZOOM_STEP_RANGE;
        Self {
            zoom_step: finite(self.zoom_step, d.zoom_step).clamp(lo, hi),
            pan_speed: finite(self.pan_speed, d.pan_speed).max(0.0),
            orbit_speed_deg: finite(self.orbit_speed_deg, d.orbit_speed_deg).max(0.0),
        }
    }
}

/// A chrome preset: which token set the editor's panels are drawn with
/// (brief U-7; the tokens are `gradiance_ui::theme`). Names only: the
/// colours live in the UI crate, the one crate that knows egui.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, bevy::reflect::Reflect,
)]
pub enum Chrome {
    /// Blender's default dark greys; the selection highlight is the scene
    /// look's accent, so a selected row and a selected body agree.
    #[default]
    Blender,
    /// Blender's greys with Blender's own blue selection highlight (the
    /// scene accent stays on the canvas).
    BlenderBlue,
    /// The UI toolkit's stock dark visuals, as the editor looked before
    /// U-7 (kept for comparison).
    Stock,
}

impl Chrome {
    /// Every preset, in menu order (the first is the default).
    pub const ALL: [Self; 3] = [Self::Blender, Self::BlenderBlue, Self::Stock];

    /// The stable name operations and review scenes use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Blender => "blender",
            Self::BlenderBlue => "blender-blue",
            Self::Stock => "stock",
        }
    }

    /// The menu label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Blender => "Blender dark",
            Self::BlenderBlue => "Blender dark, blue selection",
            Self::Stock => "Stock (before U-7)",
        }
    }

    /// The preset called `name`, if any.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// The chrome preset in use (Settings > Interface, `chrome.apply`).
///
/// **Workstation config** (CLAUDE.md invariant 4): it describes the person
/// at the editor, not the scene, so it is never undone. Like
/// [`ToolDefaults`] it is not written into the scene file either: a scene
/// opened from someone else must not change your panels. A workstation file
/// is brief U-10's (dock layouts); the preset moves there with them.
#[derive(
    Resource,
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
#[serde(default)]
pub struct ChromeSettings {
    /// The active preset.
    pub chrome: Chrome,
}

/// How the editor shows physical quantities: degrees or radians, and the
/// length scale (Settings > Interface, `units.display`; brief 5-3).
///
/// **Workstation config** (CLAUDE.md invariant 4), classified like
/// [`ChromeSettings`]: it describes the person reading the fields, not the
/// scene, so it is never undone, and it is not written into the scene file
/// either: opening someone's scene must not switch your fields to their
/// units. Display only: authored values, the save format, scripts and
/// operations stay base SI. It moves to the workstation file with
/// [`ChromeSettings`] (brief U-10).
#[derive(
    Resource,
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    bevy::reflect::Reflect,
)]
#[serde(default)]
pub struct UnitSettings {
    /// The display preference (angle unit, length scale).
    pub prefs: gradiance_units::UnitPrefs,
}

/// Tool-creation defaults (editor configuration, the Config seam).
///
/// Non-authored, non-persisted workstation preferences the tools consult at
/// commit time — like [`DebugSettings`], these are not scene state.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, bevy::reflect::Reflect)]
pub struct ToolDefaults {
    /// New sliders get travel limits `[0, drag length]` — the drag *draws*
    /// the allowed travel. Off = unlimited sliders (the old behavior).
    pub slider_limits: bool,
    /// The box tool draws by three clicks — two set one edge (length and
    /// rotation), the third sets the height to either side — instead of a
    /// two-corner drag.
    pub box_three_point: bool,
}

impl Default for ToolDefaults {
    fn default() -> Self {
        Self {
            slider_limits: true,
            box_three_point: false,
        }
    }
}

/// Simulation tuning (the Algodoo-style "Simulation" settings tab).
///
/// Authored/persisted like the grid; the physics seam applies changes to
/// the engine (`Gravity`, physics clock speed) — UI never touches avian.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
// Container default: a partial record (a review option's settings
// override, `scene::review::SettingsOverride`) fills the rest from `Default`.
#[serde(default)]
pub struct SimSettings {
    /// World gravity.
    pub gravity: Acceleration2,
    /// Simulation speed multiplier (1 = realtime).
    pub speed: f32,
    /// Solver substeps per physics step (stiffness/accuracy knob).
    #[serde(default = "default_substeps")]
    pub substeps: u32,
    /// Coulomb friction against the back plane (top-down mode: gravity
    /// points "into the screen", so bodies rub on the surface behind
    /// them). `0` = off. Set from the background right-click menu.
    #[serde(default)]
    pub plane_friction: f32,
    /// Physics steps per second (the fixed timestep).
    #[serde(default = "default_timestep_hz")]
    pub timestep_hz: Frequency,
    /// Air density for the drag forces (brief N-4). Areal, like every 2D
    /// density here: kg/m³ over the world's 1 m depth, so a body of density
    /// 1 kg/m² weighs as much as its own area of this air at 1 kg/m².
    /// The default is air at sea level; it does nothing until a drag
    /// coefficient is set.
    pub air_density: Density,
    /// Quadratic (pressure) drag coefficient `Cd`: a body moving at `v`
    /// through the air feels `½ ρ Cd w v²` against its motion, `w` its width
    /// across the relative air velocity. `0` = off (the default), `1` about a
    /// flat plate face-on.
    pub drag_coefficient: f32,
    /// Linear (viscous) drag, in N·s/m²: a body feels `c w v` against its
    /// motion, `w` its width across the relative air velocity. `0` = off
    /// (the default).
    pub linear_drag: f32,
    /// Wind: the air's velocity (m/s). Drag acts on the velocity relative
    /// to it, so with drag on a free body drifts until it moves with the
    /// wind; with both drag terms at 0 the wind does nothing.
    pub wind: Velocity2,
}

fn default_substeps() -> u32 {
    SimSettings::default().substeps
}

fn default_timestep_hz() -> Frequency {
    SimSettings::default().timestep_hz
}

impl Default for SimSettings {
    fn default() -> Self {
        Self {
            gravity: Acceleration2(Vec2::new(0.0, -10.0)),
            speed: 1.0,
            plane_friction: 0.0,
            substeps: 6,
            timestep_hz: Frequency(60.0),
            air_density: Density(Self::SEA_LEVEL_AIR_DENSITY),
            drag_coefficient: 0.0,
            linear_drag: 0.0,
            wind: Velocity2(Vec2::ZERO),
        }
    }
}

impl SimSettings {
    /// Direction reported for zero gravity, where the angle is undefined:
    /// straight down, so raising the magnitude from zero falls the usual way.
    pub const DEFAULT_GRAVITY_ANGLE_DEG: f32 = -90.0;

    /// Air at sea level, 1.2 kg/m³, over the world's 1 m depth (kg/m²).
    pub const SEA_LEVEL_AIR_DENSITY: f32 = 1.2;

    /// Gravity as `(angle, magnitude)`: the angle in degrees counter-clockwise
    /// from +x (straight down is −90°), the magnitude in m/s². Zero gravity
    /// reports [`DEFAULT_GRAVITY_ANGLE_DEG`](Self::DEFAULT_GRAVITY_ANGLE_DEG).
    ///
    /// The gravity-direction widget edits these two numbers; the seam is still
    /// the `gravity` vector, so one settled edit is one undo step.
    #[must_use]
    pub fn gravity_polar(&self) -> (f32, f32) {
        let gravity = self.gravity.value();
        let magnitude = gravity.length();
        if magnitude <= f32::EPSILON {
            return (Self::DEFAULT_GRAVITY_ANGLE_DEG, 0.0);
        }
        (gravity.y.atan2(gravity.x).to_degrees(), magnitude)
    }

    /// The gravity vector for `(angle_deg, magnitude)` — the inverse of
    /// [`gravity_polar`](Self::gravity_polar). A negative magnitude is
    /// treated as zero.
    #[must_use]
    pub fn gravity_from_polar(angle_deg: f32, magnitude: f32) -> Acceleration2 {
        Acceleration2(Vec2::from_angle(angle_deg.to_radians()) * magnitude.max(0.0))
    }
}

/// Rendering style (the "Rendering" settings tab).
///
/// Consumed by `render::toon` via change detection — the toon banding is
/// a shader parameter, so toggling styles never rebuilds materials.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
// Container default: a partial record (a review option's settings
// override, `scene::review::SettingsOverride`) fills the rest from `Default`.
#[serde(default)]
pub struct RenderSettings {
    /// The scene look ([`Look`](crate::theme::Look)): it tones the body
    /// palette and picks the overlay ink. Applying a look (`look.apply`)
    /// also resets these settings, the lighting and the scenery to the
    /// look's preset; setting only this field re-tones colours and keeps
    /// the rest.
    pub look: crate::theme::Look,
    /// Toon shading (the ramp, the dither, the face relief). Off = smooth
    /// matte PBR.
    pub toon_enabled: bool,
    /// The colour ramp lighting is looked up in (brief 4b-6): stops over
    /// the lighting level `0..1`, each a tone that tints every body the
    /// same way (`ramp` module docs). Each look ships one.
    pub ramp: crate::ramp::ColorRamp,
    /// Band-edge dither: how far (in lighting levels) to either side of a
    /// stop edge pixels pick one tone or the other in an ordered (Bayer
    /// 4x4) screen pattern, so a band edge is a pattern, not a hard line.
    /// 0 = hard edges.
    #[serde(default = "default_dither")]
    pub dither: f32,
    /// Screen pixels per dither cell: 1 is a fine grain, 3 or 4 a chunky,
    /// pixel-art one. The outline and silhouette stay crisp.
    #[serde(default = "default_dither_scale")]
    pub dither_scale: f32,
    /// Rim-light strength (0 = none) — silhouettes read better in toon.
    pub rim_strength: f32,
    /// Lit/albedo ratio mapped to the top of the ramp. Raise it if
    /// everything lands in the top band; lower it if the scene looks dark.
    #[serde(default = "default_white_point")]
    pub white_point: f32,
    /// Quantized specular glint strength (0 = none, the matte default): a
    /// hard, two-step toon highlight in the strongest light's hue (brief
    /// 4b-5). A lit patch belongs in the ramp instead.
    #[serde(default = "default_specular")]
    pub specular: f32,
    /// Face relief (0 = flat faces): how far a front face's normal tilts
    /// at its outline, toward the outline (1 = a quarter round, edge-on at
    /// the outline). The tilt comes from the shape: the distance to its
    /// outline (brief 4b-6), so a circle reads as a rounded body and a box
    /// as a slab with lit edges. Lighting only: the shape and the physics
    /// stay flat.
    #[serde(default = "default_relief")]
    pub relief: f32,
    /// How far in from the outline the relief reaches, as a fraction of
    /// the shape's half-thickness (its deepest point): 1 rounds a circle
    /// into a dome, smaller keeps a flat middle and rounds the edges.
    #[serde(default = "default_bevel")]
    pub bevel: f32,
}

fn default_white_point() -> f32 {
    RenderSettings::default().white_point
}

fn default_specular() -> f32 {
    RenderSettings::default().specular
}

fn default_relief() -> f32 {
    RenderSettings::default().relief
}

fn default_dither() -> f32 {
    RenderSettings::default().dither
}

fn default_dither_scale() -> f32 {
    RenderSettings::default().dither_scale
}

fn default_bevel() -> f32 {
    RenderSettings::default().bevel
}

impl Default for RenderSettings {
    /// The default look's preset.
    fn default() -> Self {
        crate::theme::Look::default().render()
    }
}

/// Editor debugging overlays and internals readouts (the "Debug"
/// settings tab). Never persisted — this is workstation state, not scene
/// state.
#[derive(Resource, Debug, Clone, PartialEq, bevy::reflect::Reflect)]
pub struct DebugSettings {
    /// Outline every collider's polygon (the *physics* view of shapes).
    pub show_colliders: bool,
    /// Draw each body's world-space AABB.
    pub show_aabbs: bool,
    /// Mark body origins (crosshair) — distinct from shape centroids
    /// after CSG reshapes.
    pub show_origins: bool,
    /// Mark joint anchors and their body links.
    pub show_joint_anchors: bool,
    /// Draw velocity vectors (and mark sleeping bodies).
    pub show_velocities: bool,
    /// Color every body's outline by its front-most collision-layer bit
    /// (the collision-set visualization, feedback 5.4).
    pub show_layers: bool,
    /// Trace each dynamic body's position at every solver substep of the
    /// most recent physics step (the substep debug view, feedback 8.3).
    pub show_substeps: bool,
    /// Vector-plot overlay of the superposed force field (arrows sampled
    /// on a screen-space grid through `physics::fields::Fields`).
    pub show_fields: bool,
    /// Draw contact points and their impulse-scaled normals.
    pub show_contacts: bool,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self {
            show_colliders: false,
            show_aabbs: false,
            show_origins: false,
            show_joint_anchors: true,
            show_velocities: false,
            show_layers: false,
            show_substeps: false,
            show_fields: false,
            show_contacts: false,
        }
    }
}

/// One configurable key light (multiple lights give colored, layered
/// shadows — depth cues on a flat scene).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct KeyLightSettings {
    /// Where the light shines *from*, as a screen-plane angle in degrees
    /// (0° = from the right, 90° = from above).
    pub azimuth_deg: f32,
    /// Out-of-plane tilt toward the viewer, degrees (0° = grazing the
    /// sandbox plane, 90° = head-on).
    pub elevation_deg: f32,
    /// Light color.
    pub color: crate::appearance::Rgba,
    /// Strength, lux.
    pub illuminance: f32,
    /// Whether this light casts shadows.
    pub shadows: bool,
}

impl Default for KeyLightSettings {
    fn default() -> Self {
        Self {
            // The studio key's direction and strength, untinted: a light
            // added in the Lighting tab lands at the toon white point.
            azimuth_deg: 125.0,
            elevation_deg: 40.0,
            color: crate::theme::WHITE_LIGHT,
            illuminance: 2_400.0,
            shadows: true,
        }
    }
}

/// Scene lighting (the "Lighting" settings tab).
///
/// Persisted with the scene like the other environment settings; the render
/// seam applies changes to the light entities / ambient / camera AO via
/// change detection — UI never touches light entities directly.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
// Container default: a partial record (a review option's settings
// override, `scene::review::SettingsOverride`) fills the rest from `Default`.
#[serde(default)]
pub struct LightingSettings {
    /// The key lights (at least one; the Lighting tab adds/removes).
    pub lights: Vec<KeyLightSettings>,
    /// Flat ambient brightness.
    pub ambient: f32,
    /// Screen-space ambient occlusion (forces MSAA off while enabled).
    pub ssao: bool,
    /// Screen-space contact shadows (crisp object-on-object darkening).
    pub contact_shadows: bool,
    /// Shadow-map resolution per cascade (power of two; higher = harder,
    /// crisper shadow edges — the scene is simple prisms, so go high).
    pub shadow_map_size: u32,
    /// How far from the camera shadows stay valid, world metres. Smaller =
    /// denser shadow texels (crisper); too small clips shadows when the
    /// view orbits.
    pub shadow_distance: Length,
}

impl Default for LightingSettings {
    /// The default look's preset.
    fn default() -> Self {
        crate::theme::Look::default().lighting()
    }
}

/// The scene's fixed backdrop planes (the "Lighting" tab's scenery half).
///
/// The **back plane** catches cast shadows behind the deepest occupied
/// layer; the **ground** is any half-plane body's infinite render. Colors
/// and visibility here; the ground's color is the body's own `Appearance`.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
// Container default: a partial record (a review option's settings
// override, `scene::review::SettingsOverride`) fills the rest from `Default`.
#[serde(default)]
pub struct ScenerySettings {
    /// Gap between the deepest occupied layer's back face and the back
    /// plane, metres (0 = touching).
    pub back_offset: Length,
    /// Back plane color.
    pub back_color: crate::appearance::Rgba,
    /// Draw the back plane (shadows need it to land somewhere).
    pub back_visible: bool,
    /// Draw half-plane grounds (colliders stay active regardless).
    pub ground_visible: bool,
    /// Vertical field of view in degrees; `0` = orthographic (exact 2D
    /// work), larger = perspective depth parallax across the layers.
    #[serde(default)]
    pub perspective_deg: f32,
}

impl Default for ScenerySettings {
    /// The default look's preset.
    fn default() -> Self {
        crate::theme::Look::default().scenery()
    }
}

#[cfg(test)]
mod tests {
    // float_cmp: the defaults are copied through exactly.
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn gravity_polar_round_trips() {
        let sim = SimSettings::default();
        let (angle, magnitude) = sim.gravity_polar();
        assert!((angle - -90.0).abs() < 1e-4, "default points down: {angle}");
        assert!((magnitude - 10.0).abs() < 1e-5);
        let back = SimSettings::gravity_from_polar(angle, magnitude);
        assert!((back - sim.gravity).value().length() < 1e-5, "{back:?}");

        for (angle, magnitude) in [(0.0, 3.0), (45.0, 9.81), (180.0, 1.0), (-135.0, 25.0)] {
            let g = SimSettings::gravity_from_polar(angle, magnitude);
            let (a, m) = SimSettings {
                gravity: g,
                ..SimSettings::default()
            }
            .gravity_polar();
            assert!((m - magnitude).abs() < 1e-4, "{magnitude} -> {m}");
            let wrapped = (a - angle + 540.0).rem_euclid(360.0) - 180.0;
            assert!(wrapped.abs() < 1e-3, "{angle} -> {a}");
        }

        let zero = SimSettings {
            gravity: Acceleration2(Vec2::ZERO),
            ..SimSettings::default()
        };
        assert_eq!(
            zero.gravity_polar(),
            (SimSettings::DEFAULT_GRAVITY_ANGLE_DEG, 0.0)
        );
        assert_eq!(
            SimSettings::gravity_from_polar(30.0, -2.0),
            Acceleration2(Vec2::ZERO)
        );
    }

    #[test]
    fn camera_settings_sanitize_into_a_usable_range() {
        let ok = CameraSettings::default();
        assert_eq!(ok.sanitized(), ok, "defaults are already sane");

        let bad = CameraSettings {
            zoom_step: 1.5,
            pan_speed: -3.0,
            orbit_speed_deg: f32::NAN,
        }
        .sanitized();
        assert_eq!(bad.zoom_step, CameraSettings::ZOOM_STEP_RANGE.1);
        assert_eq!(bad.pan_speed, 0.0);
        assert_eq!(bad.orbit_speed_deg, ok.orbit_speed_deg);
        assert_eq!(
            CameraSettings {
                zoom_step: 0.0,
                ..ok
            }
            .sanitized()
            .zoom_step,
            CameraSettings::ZOOM_STEP_RANGE.0,
            "a zero step would zoom to the limit in one notch"
        );
    }
}
