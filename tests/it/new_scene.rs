//! File ▸ New keeps the person's workstation (brief Q-3): `scene.new`
//! empties the scene and resets the scene-content settings, but leaves the
//! grid, snap, camera feel and saved workspaces as they were, and undo
//! leaves them alone too. A loaded file still brings its own grid.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{body_count, box_record, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::domain::settings::{GridSettings, SimSettings, SnapConfig};
use gradiance::domain::workspace::Workspaces;
use gradiance::op::invoke;
use gradiance::prelude::*;
use gradiance::scene::EnvironmentRecord;
use gradiance::units::{Acceleration2, Length};
use serde_json::json;

/// The three workstation values the test sets: grid spacing, rotation snap
/// step, active workspace.
fn workstation(app: &App) -> (f32, f32, String) {
    let world = app.world();
    (
        world.resource::<GridSettings>().spacing.0,
        world.resource::<SnapConfig>().rotation_step_deg,
        world
            .get_resource::<Workspaces>()
            .map(|w| w.active.clone())
            .unwrap_or_default(),
    )
}

#[test]
fn new_scene_keeps_the_workstation_and_resets_the_content() {
    let mut app = paused_app();
    app.world_mut().write_message(SpawnBodyIntent {
        record: box_record(Vec2::ZERO, 1.0, 1.0),
    });
    app.update();
    app.world_mut().resource_mut::<GridSettings>().spacing = Length(0.37);
    app.world_mut()
        .resource_mut::<SnapConfig>()
        .rotation_step_deg = 7.0;
    app.insert_resource(Workspaces {
        active: "Mine".into(),
        ..default()
    });
    app.world_mut().resource_mut::<SimSettings>().gravity = Acceleration2(Vec2::new(0.0, -3.0));
    step(&mut app, 3);
    let mine = workstation(&app);

    invoke(app.world_mut(), "scene.new", json!({})).expect("new");
    step(&mut app, 3);
    assert_eq!(body_count(&mut app), 0, "the bodies are gone");
    assert_eq!(
        *app.world().resource::<SimSettings>(),
        SimSettings::default(),
        "the scene content is the defaults'"
    );
    assert_eq!(workstation(&app), mine, "the workstation stays");

    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(body_count(&mut app), 1, "undo brings the body back");
    assert!(
        (app.world().resource::<SimSettings>().gravity.0.y + 3.0).abs() < 1e-6,
        "and the scene's gravity"
    );
    assert_eq!(workstation(&app), mine, "undo leaves the workstation alone");

    // Open still installs the file's own grid.
    let mut scene = gradiance::command::ops::scene::empty_scene();
    scene.environment = EnvironmentRecord {
        grid: GridSettings {
            spacing: Length(0.9),
            ..default()
        },
        ..default()
    };
    app.world_mut().write_message(LoadSceneIntent { scene });
    step(&mut app, 2);
    assert!(
        (app.world().resource::<GridSettings>().spacing.0 - 0.9).abs() < 1e-6,
        "a loaded file brings its grid"
    );
}
