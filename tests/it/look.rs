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
    op(
        &mut app,
        "render.set",
        json!({"path": "ramp.stops[1].position", "value": 0.4}),
    )
    .expect("sets");
    op(
        &mut app,
        "render.set",
        json!({"path": "ramp.stops[0].color.g", "value": 0.1}),
    )
    .expect("sets");
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
    let ramp = &world.resource::<RenderSettings>().ramp;
    assert!((ramp.stops[1].position - 0.4).abs() < 1e-6);
    assert!((ramp.stops[0].color.g - 0.1).abs() < 1e-6);
    assert!((world.resource::<LightingSettings>().lights[0].elevation_deg - 20.0).abs() < 1e-6);
    assert!((world.resource::<ScenerySettings>().perspective_deg - 15.0).abs() < 1e-6);
    assert!(op(&mut app, "render.set", json!({"path": "nope", "value": 1})).is_err());
}

/// The chrome preset (brief U-7) is workstation config: `chrome.apply`
/// switches it with no undo step, it is not saved with the scene, and an
/// unknown name lists the presets.
#[test]
fn the_chrome_preset_is_workstation_config() {
    use gradiance::domain::settings::{Chrome, ChromeSettings};
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 0, "w": 1, "h": 1}),
    )
    .expect("spawns");
    assert_eq!(
        app.world().resource::<ChromeSettings>().chrome,
        Chrome::Blender
    );
    let before = undo_depth(&app);
    op(&mut app, "chrome.apply", json!({"name": "blender-blue"})).expect("applies");
    assert_eq!(
        app.world().resource::<ChromeSettings>().chrome,
        Chrome::BlenderBlue
    );
    assert_eq!(undo_depth(&app), before, "no undo step");
    let saved = gradiance::scene::EnvironmentRecord::capture(app.world());
    let text = format!("{saved:?}").to_lowercase();
    assert!(!text.contains("chrome"), "not in the scene file: {text}");
    let err = op(&mut app, "chrome.apply", json!({"name": "neon"})).expect_err("unknown");
    assert!(
        err.contains("blender-blue") && err.contains("stock"),
        "{err}"
    );
    assert_eq!(
        op(&mut app, "chrome.list", json!({})).expect("lists"),
        Some(json!(["blender", "blender-blue", "stock"]))
    );
}

/// The colour ramp (brief 4b-6) is scene content: `render.ramp` replaces it
/// in one undo step (stops sorted, colours clamped), undo restores the
/// look's ramp, and a malformed stop list names what is wrong. The Lisp
/// verb takes the same flat stop list.
#[test]
fn the_ramp_is_replaced_in_one_undo_step() {
    use gradiance::domain::ramp::RampInterp;
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 0, "w": 1, "h": 1}),
    )
    .expect("spawns");
    let before = undo_depth(&app);
    op(
        &mut app,
        "render.ramp",
        json!({"interp": "linear", "stops": [0.7, 0.5, 0.5, 0.5, 0.0, 0.2, 0.1, 1.5]}),
    )
    .expect("replaces");
    let ramp = app.world().resource::<RenderSettings>().ramp.clone();
    assert_eq!(ramp.interp, RampInterp::Linear);
    assert_eq!(ramp.stops.len(), 2);
    assert!(ramp.stops[0].position.abs() < f32::EPSILON, "sorted");
    assert!(
        (ramp.stops[0].color.b - 1.0).abs() < f32::EPSILON,
        "clamped"
    );
    assert_eq!(undo_depth(&app), before + 1, "one step");
    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(
        app.world().resource::<RenderSettings>().ramp,
        Look::Studio.ramp()
    );
    for stops in [json!([]), json!([0.0, 0.5, 0.5])] {
        let err = op(
            &mut app,
            "render.ramp",
            json!({"interp": "constant", "stops": stops}),
        )
        .expect_err("malformed");
        assert!(err.contains("position r g b"), "{err}");
    }
    let err = op(
        &mut app,
        "render.ramp",
        json!({"interp": "smooth", "stops": [0.0, 0.5, 0.5, 0.5]}),
    )
    .expect_err("unknown interp");
    assert!(err.contains("linear"), "{err}");
}

/// A ramp-editor gesture writes the ramp in place every frame (the config
/// seam, as the Settings window does); the settled result is one undo step,
/// however many frames the drag took (brief 4b-6).
#[test]
fn a_ramp_drag_over_many_frames_is_one_undo_step() {
    let mut app = paused_app();
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 0, "w": 1, "h": 1}),
    )
    .expect("spawns");
    let before = undo_depth(&app);
    for frame in 1..=8 {
        let mut render = app.world_mut().resource_mut::<RenderSettings>();
        render.ramp.move_stop(1, 0.3 + frame as f32 * 0.01);
        step(&mut app, 1);
    }
    step(&mut app, 3);
    assert_eq!(undo_depth(&app), before + 1, "one step for the gesture");
    undo(&mut app);
    step(&mut app, 2);
    assert_eq!(
        app.world().resource::<RenderSettings>().ramp,
        Look::Studio.ramp()
    );
}
