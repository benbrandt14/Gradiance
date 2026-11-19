//! The unit display (brief 5-3): `units.display` picks how fields show
//! angles and lengths. It is workstation config: no undo step, untouched by
//! undo, not saved with the scene, and display only (values stay SI).

use crate::harness::{paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::settings::UnitSettings;
use gradiance::op::invoke;
use gradiance::units::{AngleUnit, LengthUnit};
use serde_json::{Value, json};

fn op(app: &mut App, id: &str, params: Value) -> Result<Option<Value>, String> {
    let value = invoke(app.world_mut(), id, params).map_err(|e| e.to_string())?;
    step(app, 3);
    Ok(value)
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

#[test]
fn the_unit_display_is_workstation_config() {
    let mut app = paused_app();
    assert_eq!(
        op(&mut app, "units.shown", json!({})),
        Ok(Some(json!(["deg", "m"]))),
        "degrees and metres by default"
    );
    op(
        &mut app,
        "body.spawn_box",
        json!({"x": 0, "y": 0, "w": 1, "h": 1}),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let before = undo_depth(&app);

    op(
        &mut app,
        "units.display",
        json!({"angle": "rad", "length": "cm"}),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let prefs = app.world().resource::<UnitSettings>().prefs;
    assert_eq!(
        (prefs.angle, prefs.length),
        (AngleUnit::Radians, LengthUnit::Centimetres)
    );
    assert_eq!(undo_depth(&app), before, "no undo step");
    assert_eq!(
        op(&mut app, "units.shown", json!({})),
        Ok(Some(json!(["rad", "cm"])))
    );

    // Undoing a scene edit leaves the display alone.
    undo(&mut app);
    step(&mut app, 3);
    assert_eq!(
        app.world().resource::<UnitSettings>().prefs.length,
        LengthUnit::Centimetres
    );

    // Not in the scene file: opening someone's scene keeps your units.
    let saved = gradiance::scene::EnvironmentRecord::capture(app.world());
    let text = format!("{saved:?}");
    assert!(!text.contains("Centimetres"), "not saved: {text}");

    let err = op(
        &mut app,
        "units.display",
        json!({"angle": "grad", "length": "m"}),
    )
    .expect_err("unknown angle unit");
    assert!(err.contains("deg") && err.contains("rad"), "{err}");
    let err = op(
        &mut app,
        "units.display",
        json!({"angle": "deg", "length": "ft"}),
    )
    .expect_err("unknown length unit");
    assert!(err.contains("cm") && err.contains("mm"), "{err}");
}
