//! Headless test harness: a windowless, renderless app with the full
//! Gradiance plugin stack, plus helpers shared by the integration suites.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use gradiance::prelude::*;

/// Builds a headless app with core/domain/command plugins installed.
pub fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.add_plugins(GradiancePlugins);
    // Flush startup so states and resources are initialized.
    app.update();
    app
}

/// A fully-specified box body record at `pos`.
pub fn box_record(pos: Vec2, width: f32, height: f32) -> BodyRecord {
    BodyRecord {
        id: StableId::new(),
        pose: PosRot { pos, rot: 0.0 },
        shape: ShapeDef::Box { width, height },
        props: PhysicalProps::default(),
        appearance: Appearance::default(),
        layers: LayerMask32::default(),
        group: None,
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
