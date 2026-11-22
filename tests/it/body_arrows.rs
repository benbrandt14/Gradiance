//! Per-body motion arrows (brief S-2b): `set-arrows` authors the flags in
//! the body's appearance (one undo step, saved); the group property
//! `arrow-velocity` reaches them too; a body without arrows saves nothing
//! new.

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::harness::{box_record, entity_of, paused_app, step, undo};
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::appearance::{Appearance, BodyArrows};
use gradiance::op::invoke;
use gradiance::prelude::*;
use serde_json::json;

fn arrows(app: &App, id: StableId) -> BodyArrows {
    app.world()
        .get::<Appearance>(entity_of(app, id).unwrap())
        .unwrap()
        .arrows
}

fn saved(app: &mut App) -> String {
    gradiance::scene::to_ron(&gradiance::scene::SceneRecord::capture(app.world_mut())).unwrap()
}

#[test]
fn set_arrows_is_one_undo_step_and_saves() {
    let mut app = paused_app();
    let records = [
        box_record(Vec2::ZERO, 1.0, 1.0),
        box_record(Vec2::new(2.0, 0.0), 1.0, 1.0),
    ];
    let ids = [records[0].id, records[1].id];
    for record in records {
        app.world_mut().write_message(SpawnBodyIntent { record });
    }
    step(&mut app, 3);
    assert!(
        !saved(&mut app).contains("arrows"),
        "no arrows, nothing saved"
    );

    let depth = app.world().resource::<HistoryInfo>().undo_depth;
    invoke(
        app.world_mut(),
        "body.set_arrows",
        json!({"i": ids[0].0.to_string(), "velocity": 1, "momentum": 0, "force": 1}),
    )
    .unwrap();
    step(&mut app, 3);
    assert_eq!(app.world().resource::<HistoryInfo>().undo_depth, depth + 1);
    assert_eq!(
        arrows(&app, ids[0]),
        BodyArrows {
            velocity: true,
            momentum: false,
            force: true
        }
    );
    assert!(saved(&mut app).contains("arrows"));
    undo(&mut app);
    step(&mut app, 2);
    assert!(arrows(&app, ids[0]).is_none(), "undo takes them off");

    // The group property reaches every member.
    let group = invoke(app.world_mut(), "group.create", json!({"targets": [0, 1]}))
        .unwrap()
        .unwrap();
    step(&mut app, 3);
    invoke(
        app.world_mut(),
        "group.set",
        json!({"group": group, "prop": "arrow-velocity", "v": 1}),
    )
    .unwrap();
    step(&mut app, 3);
    assert!(ids.iter().all(|id| arrows(&app, *id).velocity));
}
