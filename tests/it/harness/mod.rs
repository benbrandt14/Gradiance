//! Headless test harness: a windowless, renderless app with the full
//! Gradiance plugin stack, plus helpers shared by the integration suites.

// This module is compiled once per test binary; not every binary uses
// every helper.
#![allow(dead_code)]

use bevy::prelude::*;
use gradiance::prelude::*;
use std::time::Duration;

/// The fixed frame duration used by [`step`] (60 fps).
pub const FRAME: Duration = gradiance::run::FRAME;

/// Builds a headless app with the full Gradiance stack (including physics)
/// and a deterministic, manually-advanced clock.
pub fn headless_app() -> App {
    headless_app_with(|_| {})
}

/// [`headless_app`] with `setup` run on the app before start-up — where the
/// binary inserts its CLI resources (`StartupScripts`, `StartupScene`). The
/// same builder `gradiance run` uses (brief R-2).
pub fn headless_app_with(setup: impl FnOnce(&mut App)) -> App {
    gradiance::run::headless_app_with(setup)
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
        depth: DepthBand::default(),
        layers: None,
        groups: Vec::new(),
        field: None,
        tracer: None,
    }
}

/// Number of live authored bodies.
pub fn body_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<Body>>()
        .iter(app.world())
        .count()
}

/// Authored joints in the world — the observable for the joint verbs.
pub fn joint_count(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<Entity, With<gradiance::domain::Joint>>()
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
