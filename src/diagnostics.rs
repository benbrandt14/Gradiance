//! Optional developer diagnostics, behind the `diagnostics` feature
//! (`cargo run --features diagnostics`).
//!
//! A maintained, delete-nothing stack built entirely on Bevy's own dev-tools
//! and diagnostic plugins — no bespoke overlays:
//!
//! - an on-screen overlay ([`DiagnosticsOverlayPlugin`]) showing FPS, frame
//!   time, entity count, and **process CPU / memory** — so live memory use is
//!   visible while authoring,
//! - a periodic terminal dump of every registered diagnostic
//!   ([`LogDiagnosticsPlugin`]), and
//! - (via the feature's `bevy/track_location`) caller `#[track_caller]`
//!   locations on change detection, so a mutation's *origin* is inspectable.
//!
//! For deep runtime profiling — per-system spans and allocation memory —
//! build with the separate `tracy` feature and connect the Tracy profiler;
//! the intent/command/sync `trace!`s emitted by [`command`](gradiance_command)
//! show up there as events.
//!
//! Additive and read-only: it never mutates authored state, has no default
//! build cost, and is never compiled into release or CI builds. Added from
//! `main` under the feature.

use bevy::dev_tools::diagnostics_overlay::{DiagnosticsOverlay, DiagnosticsOverlayPlugin};
use bevy::diagnostic::{
    EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin,
    SystemInformationDiagnosticsPlugin,
};
use bevy::prelude::*;

/// Wires Bevy's dev-tools overlay and diagnostic plugins.
pub struct DiagnosticsPlugin;

impl Plugin for DiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // On-screen overlay; the entities it draws are spawned below.
            DiagnosticsOverlayPlugin,
            // Data sources for the overlay and the terminal dump. (FPS /
            // frame time are already registered by `render`.)
            EntityCountDiagnosticsPlugin::default(),
            SystemInformationDiagnosticsPlugin,
            // Periodic terminal dump of every registered diagnostic.
            LogDiagnosticsPlugin::default(),
        ))
        .add_systems(Startup, spawn_overlay);
    }
}

/// Spawns the on-screen overlay: FPS, frame time, entity count, and process
/// CPU / memory — memory use foremost, since it grows with scene complexity.
fn spawn_overlay(mut commands: Commands) {
    commands.spawn(DiagnosticsOverlay::new(
        "Gradiance",
        vec![
            FrameTimeDiagnosticsPlugin::FPS.into(),
            FrameTimeDiagnosticsPlugin::FRAME_TIME.into(),
            EntityCountDiagnosticsPlugin::ENTITY_COUNT.into(),
            SystemInformationDiagnosticsPlugin::PROCESS_CPU_USAGE.into(),
            SystemInformationDiagnosticsPlugin::PROCESS_MEM_USAGE.into(),
        ],
    ));
}
