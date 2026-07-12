//! Headless test harness: a windowless, renderless app with the full
//! Gradiance plugin stack, plus helpers shared by the integration suites.

// This module is compiled once per test binary; not every binary uses
// every helper.
#![allow(dead_code)]

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use gradiance::prelude::*;
use std::time::Duration;

/// The fixed frame duration used by [`step`] (60 fps).
pub const FRAME: Duration = Duration::from_micros(16_667);

/// Builds a headless app with the full Gradiance stack (including physics)
/// and a deterministic, manually-advanced clock.
pub fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        TransformPlugin,
        bevy::input::InputPlugin,
    ));
    app.add_plugins(GradiancePlugins);
    // Advance exactly one frame of virtual time per update, regardless of
    // wall-clock speed, so physics assertions are reproducible.
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    // Without a runner, plugin `finish`/`cleanup` never run — avian
    // registers diagnostics resources there, so run them explicitly.
    app.finish();
    app.cleanup();
    // Flush startup so states and resources are initialized.
    app.update();
    app
}

/// Advances the app by `frames` fixed 60 fps frames.
pub fn step(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

/// A headless app with the simulation paused — for tests that assert
/// exact authored state without physics drift.
pub fn paused_app() -> App {
    let mut app = headless_app();
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    app
}

/// A fully-specified box body record at `pos`.
pub fn box_record(pos: Vec2, width: f32, height: f32) -> BodyRecord {
    BodyRecord {
        id: StableId::new(),
        pose: PosRot { pos, rot: 0.0 },
        shape: ShapeDef::Box { width, height },
        physics: BodyPhysics::default(),
        appearance: Appearance::default(),
        layers: LayerMask32::default(),
        groups: Vec::new(),
        magnet: None,
    }
}

/// Number of live authored bodies.
pub fn body_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<Body>>()
        .iter(app.world())
        .count()
}

/// Resolves a stable id to its live entity via the `IdIndex`.
pub fn entity_of(app: &App, id: StableId) -> Option<Entity> {
    app.world().resource::<IdIndex>().entity(id)
}

/// Sends an undo intent and runs a frame.
pub fn undo(app: &mut App) {
    app.world_mut().write_message(UndoIntent);
    app.update();
}

/// Sends a redo intent and runs a frame.
pub fn redo(app: &mut App) {
    app.world_mut().write_message(RedoIntent);
    app.update();
}
