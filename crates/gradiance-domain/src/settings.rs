//! Authored editor settings: grid and snapping configuration.
//!
//! These are scene-level settings (Algodoo persists grid setup with the
//! scene), so they live in `domain` and serialize into the save file's
//! environment section.

use bevy::math::Vec2;
use bevy::prelude::Resource;
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
    /// Base cell size in world pixels (display adapts by powers of two).
    pub spacing: f32,
    /// Grid origin in world space.
    pub origin: Vec2,
    /// Grid rotation in radians.
    pub rotation: f32,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            visible: true,
            snap_enabled: false,
            system: GridSystem::Cartesian,
            spacing: 100.0,
            origin: Vec2::ZERO,
            rotation: 0.0,
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
}

impl Default for SnapConfig {
    fn default() -> Self {
        Self {
            objects_enabled: true,
            max_screen_distance: 12.0,
            sources: SnapSources::default(),
            rotation_step_deg: 15.0,
        }
    }
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
}

impl Default for ToolDefaults {
    fn default() -> Self {
        Self {
            slider_limits: true,
        }
    }
}

/// Simulation tuning (the Algodoo-style "Simulation" settings tab).
///
/// Authored/persisted like the grid; the physics seam applies changes to
/// the engine (`Gravity`, physics clock speed) — UI never touches avian.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct SimSettings {
    /// World gravity, px/s².
    pub gravity: Vec2,
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
    pub timestep_hz: f32,
}

fn default_substeps() -> u32 {
    SimSettings::default().substeps
}

fn default_timestep_hz() -> f32 {
    SimSettings::default().timestep_hz
}

impl Default for SimSettings {
    fn default() -> Self {
        Self {
            gravity: Vec2::new(0.0, -1000.0),
            speed: 1.0,
            plane_friction: 0.0,
            substeps: 6,
            timestep_hz: 60.0,
        }
    }
}

/// Rendering style (the "Rendering" settings tab).
///
/// Consumed by `render::toon` via change detection — the toon banding is
/// a shader parameter, so toggling styles never rebuilds materials.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct RenderSettings {
    /// Quantize lighting into bands (toon look). Off = smooth matte PBR.
    pub toon_enabled: bool,
    /// Number of light bands when toon shading is on.
    pub bands: u32,
    /// Rim-light strength (0 = none) — silhouettes read better in toon.
    pub rim_strength: f32,
    /// Lit/albedo ratio mapped to the top band. Raise it if everything
    /// saturates into one flat band; lower it if the scene looks dark.
    #[serde(default = "default_white_point")]
    pub white_point: f32,
    /// Quantized specular glint strength (0 = none) — a hard toon
    /// highlight in the key lights' hues; reads best on curved walls.
    #[serde(default = "default_specular")]
    pub specular: f32,
}

fn default_white_point() -> f32 {
    RenderSettings::default().white_point
}

fn default_specular() -> f32 {
    RenderSettings::default().specular
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            toon_enabled: true,
            bands: 4,
            rim_strength: 0.25,
            white_point: 1.3,
            specular: 0.3,
        }
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
            azimuth_deg: 120.0,
            elevation_deg: 32.0,
            color: crate::appearance::Rgba::rgb(1.0, 1.0, 1.0),
            illuminance: 12_000.0,
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
    /// How far from the camera shadows stay valid, world pixels. Smaller =
    /// denser shadow texels (crisper); too small clips shadows when the
    /// view orbits.
    pub shadow_distance: f32,
}

impl Default for LightingSettings {
    fn default() -> Self {
        // The single-light default matches the original hard-coded key
        // light: from (-400, 700) in-plane, lifted toward the viewer.
        Self {
            lights: vec![KeyLightSettings::default()],
            ambient: 300.0,
            ssao: false,
            contact_shadows: true,
            shadow_map_size: 4096,
            shadow_distance: 8_000.0,
        }
    }
}

/// The scene's fixed backdrop planes (the "Lighting" tab's scenery half).
///
/// The **back plane** catches cast shadows behind the deepest occupied
/// layer; the **ground** is any half-plane body's infinite render. Colors
/// and visibility here; the ground's color is the body's own `Appearance`.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize, bevy::reflect::Reflect)]
pub struct ScenerySettings {
    /// Gap between the deepest occupied layer's back face and the back
    /// plane, world pixels (0 = touching).
    pub back_offset: f32,
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
    fn default() -> Self {
        Self {
            back_offset: 20.0,
            back_color: crate::appearance::Rgba::rgb(0.82, 0.83, 0.85),
            back_visible: true,
            ground_visible: true,
            perspective_deg: 0.0,
        }
    }
}
