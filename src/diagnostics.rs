//! Optional developer diagnostics, behind the `diagnostics` feature
//! (`cargo run --features diagnostics`).
//!
//! A maintained, delete-nothing stack built entirely on Bevy's own dev-tools
//! and diagnostic plugins — no bespoke overlays:
//!
//! - an on-screen FPS + frame-time-graph overlay ([`FpsOverlayPlugin`]),
//! - entity-count and CPU/memory diagnostics feeding it and the log,
//! - a periodic terminal dump of every registered diagnostic, and
//! - (via the feature's `bevy/track_location`) caller `#[track_caller]`
//!   locations on change detection, so a mutation's *origin* is inspectable.
//!
//! Additive and read-only: it never mutates authored state, has no default
//! build cost, and is never compiled into release or CI builds. Added from
//! `main` under the feature. Intent/command/sync tracing lives in
//! [`command`](gradiance_command) and is always on via `RUST_LOG`.

use bevy::dev_tools::fps_overlay::FpsOverlayPlugin;
use bevy::diagnostic::{
    EntityCountDiagnosticsPlugin, LogDiagnosticsPlugin, SystemInformationDiagnosticsPlugin,
};
use bevy::prelude::*;

/// Wires Bevy's dev-tools overlay and diagnostic plugins.
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // On-screen FPS + frame-time graph. Self-adds `FrameTimeDiagnosticsPlugin`
            // only if absent — `render` already adds it, so this reuses it.
            FpsOverlayPlugin::default(),
            // Extra data sources for the overlay and the terminal dump.
            EntityCountDiagnosticsPlugin::default(),
            SystemInformationDiagnosticsPlugin,
            // Periodic terminal dump of every registered diagnostic.
            LogDiagnosticsPlugin::default(),
        ));
    }
}
