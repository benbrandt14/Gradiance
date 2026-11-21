//! Physics debug toggles and the schedule dump (brief 6-3).
//!
//! Each `debug.*` operation sets its `DebugSettings` switch, the engine
//! switches reach avian's `PhysicsGizmos` config in `GizmoConfigStore`,
//! nothing enters undo history, and the View > Debug rows (which write
//! `InvokeOp::with(id, {"on": …})`) take the same path. The dump lists
//! known systems in their constrained order.

use crate::harness::{headless_app, step};
use avian2d::prelude::PhysicsGizmos;
use bevy::gizmos::config::GizmoConfigStore;
use bevy::prelude::*;
use gradiance::command::HistoryInfo;
use gradiance::domain::settings::DebugSettings;
use gradiance::interaction::debug_ops::DEBUG_TOGGLES;
use gradiance::op::{InvokeOp, invoke};
use serde_json::json;

/// Whether the engine overlay group is enabled, and which colours are set:
/// (enabled, colliders, contacts, joints, aabbs).
fn engine(app: &App) -> (bool, bool, bool, bool, bool) {
    let store = app.world().resource::<GizmoConfigStore>();
    let (config, gizmos) = store.config::<PhysicsGizmos>();
    (
        config.enabled,
        gizmos.collider_color.is_some(),
        gizmos.contact_point_color.is_some(),
        gizmos.joint_anchor_color.is_some(),
        gizmos.aabb_color.is_some(),
    )
}

fn undo_depth(app: &App) -> usize {
    app.world().resource::<HistoryInfo>().undo_depth
}

/// A box hinged to a second box and pinned to the world: every engine
/// overlay has something to draw.
fn jointed_scene(app: &mut App) {
    for (x, y) in [(0.0, 1.0), (1.0, 1.0)] {
        let r = invoke(
            app.world_mut(),
            "body.spawn_box",
            json!({"x": x, "y": y, "w": 0.8, "h": 0.4}),
        );
        assert!(r.is_ok(), "{r:?}");
        step(app, 1);
    }
    for (b, x) in [(1, 0.5), (-1, -0.3)] {
        let r = invoke(
            app.world_mut(),
            "joint.hinge",
            json!({"a": 0, "b": b, "x": x, "y": 1.0}),
        );
        assert!(r.is_ok(), "{r:?}");
        step(app, 1);
    }
    let r = invoke(
        app.world_mut(),
        "body.spawn_ground",
        json!({"x": 0, "y": -1, "angle": 0}),
    );
    assert!(r.is_ok(), "{r:?}");
    step(app, 2);
}

#[test]
fn every_toggle_is_off_by_default_and_the_engine_group_disabled() {
    let app = headless_app();
    let debug = app.world().resource::<DebugSettings>();
    for row in DEBUG_TOGGLES {
        assert!(!(row.get)(debug), "{} starts off", row.id);
    }
    assert_eq!(engine(&app), (false, false, false, false, false));
}

#[test]
fn each_operation_flips_its_switch_and_the_gizmo_config_without_an_undo_step() {
    let mut app = headless_app();
    jointed_scene(&mut app);
    let depth = undo_depth(&app);
    for row in DEBUG_TOGGLES {
        let r = invoke(app.world_mut(), row.id, json!({"on": true}));
        assert!(r.is_ok(), "{}: {r:?}", row.id);
        step(&mut app, 2);
        assert!(
            (row.get)(app.world().resource::<DebugSettings>()),
            "{} on",
            row.id
        );
        let (enabled, colliders, contacts, joints, aabbs) = engine(&app);
        let expect = match row.id {
            "debug.colliders" => colliders,
            "debug.contacts" => contacts,
            "debug.joints" => joints,
            "debug.aabbs" => aabbs,
            // avian 0.8 has no velocity gizmo: Gradiance's own overlay.
            "debug.velocities" => true,
            other => panic!("unexpected toggle {other}"),
        };
        assert!(expect, "{} reaches the gizmo config", row.id);
        assert!(enabled, "{}: the group is on while a switch is", row.id);
        assert_eq!(undo_depth(&app), depth, "{} records no undo step", row.id);
    }
    // Everything on, a few frames: avian's draw systems run headless on a
    // jointed scene without failing.
    step(&mut app, 5);
    for row in DEBUG_TOGGLES {
        let r = invoke(app.world_mut(), row.id, json!({"on": false}));
        assert!(r.is_ok(), "{}: {r:?}", row.id);
    }
    step(&mut app, 2);
    assert_eq!(engine(&app), (false, false, false, false, false));
    assert_eq!(undo_depth(&app), depth);
}

#[test]
fn a_menu_row_invocation_flips_the_same_config() {
    let mut app = headless_app();
    // What View > Debug writes on a click.
    app.world_mut()
        .write_message(InvokeOp::with("debug.colliders", json!({"on": true})));
    step(&mut app, 2);
    assert!(app.world().resource::<DebugSettings>().show_colliders);
    assert_eq!(engine(&app), (true, true, false, false, false));
    app.world_mut()
        .write_message(InvokeOp::with("debug.colliders", json!({"on": false})));
    step(&mut app, 2);
    assert_eq!(engine(&app), (false, false, false, false, false));
}

#[test]
fn the_lisp_verb_flips_the_switch() {
    let mut app = headless_app();
    let r = gradiance::op::invoke(app.world_mut(), "debug.joints", json!({"on": true}));
    assert!(r.is_ok(), "{r:?}");
    step(&mut app, 2);
    assert!(app.world().resource::<DebugSettings>().show_joints);
    app.world_mut()
        .resource_mut::<gradiance::script::bridge::ScriptInputs>()
        .submit("(debug-joints #f)");
    step(&mut app, 3);
    assert!(!app.world().resource::<DebugSettings>().show_joints);
}

#[test]
fn toggling_leaves_a_hidden_body_hidden() {
    // avian resets every rigid body's `Visibility` when the gizmo store's
    // change flag is set; the toggles write the store around it, so a ground
    // the Scenery settings hid stays hidden. Headless, avian's plugin (and
    // its visibility system) is absent: this pins only that the toggles
    // themselves never touch a body's visibility; the windowed half is the
    // `bypass_change_detection` in `debug_gizmos::apply_debug_settings`.
    let mut app = headless_app();
    jointed_scene(&mut app);
    let ground = {
        let world = app.world_mut();
        let mut q = world.query_filtered::<Entity, With<avian2d::prelude::RigidBody>>();
        q.iter(world).last()
    };
    let Some(ground) = ground else {
        panic!("the scene has bodies");
    };
    app.world_mut()
        .entity_mut(ground)
        .insert(Visibility::Hidden);
    for row in DEBUG_TOGGLES {
        let r = invoke(app.world_mut(), row.id, json!({"on": true}));
        assert!(r.is_ok(), "{r:?}");
        step(&mut app, 2);
    }
    assert_eq!(
        app.world().get::<Visibility>(ground),
        Some(&Visibility::Hidden)
    );
}

#[test]
fn the_schedule_dump_lists_known_systems_in_order() {
    let mut app = headless_app();
    let all = gradiance::schedule_dump::dump(app.world_mut(), None);
    let Ok(all) = all else {
        panic!("{all:?}");
    };
    for section in [
        "# First ",
        "# PreUpdate ",
        "# FixedUpdate ",
        "# Update ",
        "# PostUpdate ",
        "# PhysicsSchedule ",
    ] {
        assert!(all.contains(section), "{section} in the dump");
    }
    // Frame order: the fixed loop sits between PreUpdate and Update.
    let at = |s: &str| all.find(s).unwrap_or(usize::MAX);
    assert!(at("# PreUpdate ") < at("# FixedUpdate "));
    assert!(at("# FixedUpdate ") < at("# Update "));
    assert!(at("# Update ") < at("# PostUpdate "));

    let post = gradiance::schedule_dump::dump(app.world_mut(), Some("postupdate"));
    let Ok(post) = post else {
        panic!("{post:?}");
    };
    assert!(post.starts_with("# PostUpdate ("), "{post}");
    assert!(!post.contains("# Update "));
    // The body sync chain, in its declared order.
    let line = |name: &str| {
        post.lines()
            .position(|l| l.trim_end().ends_with(name))
            .unwrap_or_else(|| panic!("{name} in PostUpdate:\n{post}"))
    };
    let colliders = line("body_sync::sync_colliders");
    let joints = line("joint_sync::sync_joints");
    let retune = line("joint_sync::retune_joints");
    assert!(colliders < joints && joints < retune);
    // The debug switch is applied in PostUpdate too.
    line("debug_gizmos::apply_debug_settings");

    let none = gradiance::schedule_dump::dump(app.world_mut(), Some("NoSuchSchedule"));
    assert!(none.is_err_and(|e| e.contains("PostUpdate")));
}
