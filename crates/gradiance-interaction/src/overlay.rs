//! The interaction-plane overlay: the single gizmo group every editor
//! overlay draws through.
//!
//! All gizmos — grid, snap glyphs, selection outlines, tool previews, joint
//! glyphs, debug overlays — live on the **interaction plane**
//! ([`PlaneFrame::XY`](gradiance_core::units::PlaneFrame::XY), the picking
//! plane) and render with a negative `depth_bias` so they always draw
//! in front of the extruded body prisms. One group replaces the former
//! default/`JointGizmos`/`SelectionGizmos` split, so "overlays sit on the
//! interaction plane, in front of the scene" holds everywhere by construction.

use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup};
use bevy::prelude::*;

/// Gizmo config group for every *interactive* editor overlay (see module
/// docs). Drawn in front of everything, including the grid.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OverlayGizmos;

/// Gizmo config group for the reference grid: in front of the scene, but
/// **behind** every interactive overlay (a selection outline or snap glyph
/// must never lose to a grid line).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct GridGizmos;

/// Registers both groups with their always-in-front configs.
pub fn install(app: &mut App) {
    app.insert_gizmo_config(
        OverlayGizmos,
        GizmoConfig {
            depth_bias: -1.0,
            ..default()
        },
    );
    app.insert_gizmo_config(
        GridGizmos,
        GizmoConfig {
            depth_bias: -0.5,
            ..default()
        },
    );
}
