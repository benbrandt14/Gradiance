//! The interaction-plane overlay: the single gizmo group every editor
//! overlay draws through.
//!
//! All gizmos — grid, snap glyphs, selection outlines, tool previews, joint
//! glyphs, debug overlays — live on the **interaction plane**
//! ([`INTERACTION_PLANE_Z`](crate::core::constants::INTERACTION_PLANE_Z), the
//! picking plane) and render with a negative `depth_bias` so they always draw
//! in front of the extruded body prisms. One group replaces the former
//! default/`JointGizmos`/`SelectionGizmos` split, so "overlays sit on the
//! interaction plane, in front of the scene" holds everywhere by construction.

use bevy::gizmos::AppGizmoBuilder;
use bevy::gizmos::config::{GizmoConfig, GizmoConfigGroup};
use bevy::prelude::*;

/// Gizmo config group for every editor overlay (see module docs).
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OverlayGizmos;

/// Registers the group with its always-in-front config.
pub fn install(app: &mut App) {
    app.insert_gizmo_config(
        OverlayGizmos,
        GizmoConfig {
            depth_bias: -1.0,
            ..default()
        },
    );
}
