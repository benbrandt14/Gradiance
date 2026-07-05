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
}

impl Default for SimSettings {
    fn default() -> Self {
        Self {
            gravity: Vec2::new(0.0, -1000.0),
            speed: 1.0,
        }
    }
}
