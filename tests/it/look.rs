//! The scene look (brief 4b-3): `look.apply` sets a look's presets in one
//! undo step, the per-setting writes resolve, and every look's colours
//! reach a body through the display tone.

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::settings::{LightingSettings, RenderSettings, ScenerySettings};
use gradiance::domain::theme::Look;
use gradiance::op::invoke;
use serde_json::{Value, json};

fn op(app: &mut App, id: &str, params: Value) -> Result<Option<Value>, String> {
    let value = invoke(app.world_mut(), id, params).map_err(|e| e.to_string())?;
    // The settings watcher records a settled edit a frame after it lands.
    step(app, 3);
    Ok(value)
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

#[test]
fn applying_a_look_sets_its_presets_in_one_undo_step() {
    let mut app = paused_app();
    // A first command seeds the history (a settings edit on an empty stack
    // only sets the baseline).
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 0, "w": 1, "h": 1}),
    )
    .expect("spawns");
    assert_eq!(app.world().resource::<RenderSettings>().look, Look::Studio);
    let before = undo_depth(&app);
    op(&mut app, "look.apply", json!({"name": "blueprint"})).expect("applies");
    let world = app.world();
    assert_eq!(
        *world.resource::<RenderSettings>(),
        Look::Blueprint.render()
    );
    assert_eq!(
        *world.resource::<LightingSettings>(),
        Look::Blueprint.lighting()
    );
    assert_eq!(
        *world.resource::<ScenerySettings>(),
        Look::Blueprint.scenery()
    );
    assert_eq!(undo_depth(&app), before + 1, "three resources, one step");
    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(
        *app.world().resource::<RenderSettings>(),
        Look::Studio.render()
    );
    assert_eq!(
        *app.world().resource::<LightingSettings>(),
        Look::Studio.lighting()
    );
}

#[test]
fn look_ops_name_their_errors_and_list_the_looks() {
    let mut app = paused_app();
    let err = op(&mut app, "look.apply", json!({"name": "neon"})).expect_err("unknown");
    assert!(err.contains("blueprint"), "{err}");
    assert_eq!(
        op(&mut app, "look.list", json!({})).expect("lists"),
        Some(json!(["studio", "blueprint", "poster"]))
    );
    op(&mut app, "render.set", json!({"path": "bands", "value": 2})).expect("sets");
    op(
        &mut app,
        "lighting.set",
        json!({"path": "lights[0].elevation_deg", "value": 20}),
    )
    .expect("sets");
    op(
        &mut app,
        "scenery.set",
        json!({"path": "perspective_deg", "value": 15}),
    )
    .expect("sets");
    let world = app.world();
    assert_eq!(world.resource::<RenderSettings>().bands, 2);
    assert!((world.resource::<LightingSettings>().lights[0].elevation_deg - 20.0).abs() < 1e-6);
    assert!((world.resource::<ScenerySettings>().perspective_deg - 15.0).abs() < 1e-6);
    assert!(op(&mut app, "render.set", json!({"path": "nope", "value": 1})).is_err());
}
