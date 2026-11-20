//! Cut / click-selection / boolean stress tests (brief J-2).

// Test-only file: unwraps are the failure mechanism.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use crate::harness::{paused_app, undo};
use avian2d::prelude::RigidBody;
use bevy::prelude::*;
use gradiance::interaction::camera::CameraScale;
use gradiance::interaction::selection::Selection;
use gradiance::prelude::*;
use gradiance::script::bridge::{ScriptInputs, ScriptLog};

fn run(app: &mut App, source: &str) {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(source);
    app.update();
    app.update();
    let log = app.world().resource::<ScriptLog>();
    if let Some(entry) = log.0.last() {
        assert!(
            entry.ok,
            "script failed: {} -> {}",
            entry.input, entry.output
        );
    }
}

fn bodies(app: &mut App) -> Vec<(Entity, Vec2)> {
    let mut q = app
        .world_mut()
        .query_filtered::<(Entity, &Transform, &ShapeDef), With<Body>>();
    q.iter(app.world())
        .filter(|(_, _, s)| !s.contains_half_plane())
        .map(|(e, t, _)| (e, t.translation.truncate()))
        .collect()
}

fn set_cursor(app: &mut App, p: Vec2) {
    app.world_mut().insert_resource(CursorWorldPos(Some(p)));
    app.world_mut().insert_resource(SnappedCursor {
        raw: Some(p),
        position: Some(p),
        kind: None,
    });
}

fn use_tool(app: &mut App, tool: ToolState) {
    app.world_mut().resource_mut::<SnapConfig>().objects_enabled = false;
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(tool);
    app.update();
}

fn drag(app: &mut App, from: Vec2, to: Vec2) {
    set_cursor(app, from);
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();
    for i in 1..=4 {
        set_cursor(app, from.lerp(to, i as f32 / 4.0));
        app.update();
    }
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .release(MouseButton::Left);
    app.update();
    app.update();
}

fn click(app: &mut App, p: Vec2) {
    drag(app, p, p);
}

fn selected(app: &App) -> Vec<Entity> {
    app.world().resource::<Selection>().iter().collect()
}

#[test]
#[ignore = "probe"]
fn probe_cut_alignment() {
    let mut app = paused_app();
    app.world_mut().insert_resource(CameraScale(0.01));
    for (label, spawn) in [
        ("box 4x1", "(spawn-box 0 1 4 1)"),
        ("box 1x1", "(spawn-box 0 1 1 1)"),
        ("circle 0.5", "(spawn-circle 0 1 0.5)"),
        ("box 8x0.2", "(spawn-box 0 1 8 0.2)"),
    ] {
        run(&mut app, spawn);
        let mut fails = Vec::new();
        for i in 0..20 {
            let x = 0.1 + i as f32 * 0.0023;
            app.world_mut()
                .resource_mut::<ScriptInputs>()
                .submit(format!("(cut {x} 3 {x} -1 0.03)"));
            app.update();
            app.update();
            let n = bodies(&mut app).len();
            if n == 2 {
                undo(&mut app);
            } else {
                fails.push((x, n));
            }
        }
        println!("{label}: fails {fails:?}");
        run(&mut app, "(delete 0)");
    }
}

#[test]
#[ignore = "probe"]
fn probe_select_after_move() {
    let mut app = paused_app();
    app.world_mut().insert_resource(CameraScale(0.01));
    run(&mut app, "(spawn-box 0 1 1 1)");
    use_tool(&mut app, ToolState::Select);
    click(&mut app, Vec2::new(0.0, 1.0));
    println!("select click: {:?}", selected(&app));
    drag(&mut app, Vec2::new(0.0, 1.0), Vec2::new(2.0, 1.5));
    println!(
        "after move: {:?} sel {:?}",
        bodies(&mut app),
        selected(&app)
    );
    click(&mut app, Vec2::new(-1.0, -0.5));
    println!("empty click: sel {:?}", selected(&app));
    click(&mut app, Vec2::new(2.0, 1.5));
    println!("click new place: sel {:?}", selected(&app));
    click(&mut app, Vec2::new(-1.0, -0.5));
    use_tool(&mut app, ToolState::Cut);
    click(&mut app, Vec2::new(2.0, 1.5));
    println!("click-through in cut tool: sel {:?}", selected(&app));
    drag(&mut app, Vec2::new(2.0, 2.5), Vec2::new(2.0, 0.0));
    println!("cut after move: {:?}", bodies(&mut app));
}

fn kinds(app: &mut App) -> Vec<String> {
    let mut q = app
        .world_mut()
        .query_filtered::<(&avian2d::prelude::RigidBody, &ShapeDef), With<Body>>();
    q.iter(app.world())
        .filter(|(_, s)| !s.contains_half_plane())
        .map(|(k, s)| {
            format!(
                "{k:?}:{}",
                gradiance::geometry::polygonize::polygonize(s).outline.len()
            )
        })
        .collect()
}

#[test]
#[ignore = "probe"]
fn probe_misc() {
    let mut app = paused_app();
    app.world_mut().insert_resource(CameraScale(0.01));
    run(&mut app, "(spawn-box 0 1 1 1)");
    use_tool(&mut app, ToolState::Select);
    drag(&mut app, Vec2::new(0.0, 1.0), Vec2::new(2.0, 1.5));
    println!("kind after move: {:?}", kinds(&mut app));
    undo(&mut app);
    crate::harness::redo(&mut app);
    println!("kind after undo/redo: {:?}", kinds(&mut app));
    // cut a ring
    run(&mut app, "(delete 0)");
    run(&mut app, "(spawn-box 0 1 2 2) (spawn-box 0 1 1 1)");
    run(&mut app, "(subtract 0 1)");
    println!("ring: {:?}", kinds(&mut app));
    run(&mut app, "(spawn-box 5 1 1 1)");
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit("(cut 0 3 0 1 0.03)");
    app.update();
    app.update();
    println!(
        "cut one side of ring: {:?} {:?}",
        kinds(&mut app),
        app.world()
            .resource::<ScriptLog>()
            .0
            .last()
            .map(|e| e.output.clone())
    );
    // cut the box at x=5 and print piece outlines
    run(&mut app, "(cut 5.1 3 5.1 -1 0.03)");
    let mut q = app
        .world_mut()
        .query_filtered::<(&Transform, &ShapeDef), With<Body>>();
    for (t, s) in q.iter(app.world()) {
        if t.translation.x > 4.0 {
            println!("piece at {:?}: {:?}", t.translation.truncate(), s);
        }
    }
    // play: then select a fallen body
    run(&mut app, "(spawn-ground 0 -1 0)");
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    for _ in 0..300 {
        app.update();
    }
    let bs = bodies(&mut app);
    println!("after play: {bs:?}");
    for tool in [ToolState::Box, ToolState::Select] {
        use_tool(&mut app, tool);
        for (e, _) in bodies(&mut app) {
            let p = bodies(&mut app).into_iter().find(|b| b.0 == e).unwrap().1;
            click(&mut app, p);
            println!("{tool:?} click {e:?} at {p:?} -> {:?}", selected(&app));
        }
    }
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Paused);
    app.update();
    use_tool(&mut app, ToolState::Select);
    for (e, p) in bodies(&mut app) {
        click(&mut app, p);
        println!("paused click {e:?} at {p:?} -> {:?}", selected(&app));
    }
}

#[test]
#[ignore = "probe"]
fn probe_ops_then_click() {
    let mut app = paused_app();
    app.world_mut().insert_resource(CameraScale(0.01));
    use_tool(&mut app, ToolState::Select);
    let probe = |app: &mut App, label: &str, p: Vec2| {
        click(app, Vec2::new(-20.0, -20.0));
        click(app, p);
        println!(
            "{label}: click {p:?} -> {:?} bodies {:?}",
            selected(app),
            kinds(app)
        );
    };
    run(&mut app, "(spawn-box 0 1 1 1) (spawn-box 0.6 1 1 1)");
    run(&mut app, "(merge 0 1)");
    probe(&mut app, "merge", Vec2::new(1.0, 1.0));
    probe(&mut app, "merge", Vec2::new(-0.4, 1.0));
    run(&mut app, "(spawn-box 0.3 1 0.4 3)");
    run(&mut app, "(subtract 0 1)");
    probe(&mut app, "subtract split", Vec2::new(1.0, 1.0));
    probe(&mut app, "subtract split", Vec2::new(-0.4, 1.0));
    probe(&mut app, "subtract gap", Vec2::new(0.3, 1.0));
    run(&mut app, "(new-scene)");
    run(&mut app, "(spawn-box 0 1 1 1)");
    use_tool(&mut app, ToolState::Select);
    drag(&mut app, Vec2::new(0.0, 1.0), Vec2::new(3.0, 1.0));
    run(&mut app, "(spawn-box 3.6 1 1 1)");
    run(&mut app, "(merge 0 1)");
    probe(&mut app, "move+merge", Vec2::new(3.9, 1.0));
    probe(&mut app, "move+merge", Vec2::new(2.6, 1.0));
    run(&mut app, "(cut 3.3 3 3.3 -1 0.03)");
    probe(&mut app, "move+merge+cut", Vec2::new(3.9, 1.0));
    probe(&mut app, "move+merge+cut", Vec2::new(2.6, 1.0));
}

// ---------- Findings (each failed before its fix) ----------

/// The real app's default zoom: 1 m = 100 px, so the cut tool's 3 px
/// stroke is 3 cm wide.
const APP_CAM_SCALE: f32 = 0.01;

fn cut_count(app: &mut App, x: f32) -> usize {
    app.world_mut()
        .resource_mut::<ScriptInputs>()
        .submit(format!("(cut {x} 3 {x} -1 0.03)"));
    app.update();
    app.update();
    bodies(app).len()
}

#[test]
fn a_thin_cut_severs_a_long_body_wherever_it_crosses() {
    // A 3 cm stroke is thinner than the contour cell of a 4 m (or 8 m)
    // body: the cut fell between two sample rows at some positions and
    // the body ignored it.
    let mut app = paused_app();
    for spawn in ["(spawn-box 0 1 4 1)", "(spawn-box 0 1 8 0.2)"] {
        run(&mut app, spawn);
        for i in 0..20 {
            let x = 0.1 + i as f32 * 0.0023;
            assert_eq!(cut_count(&mut app, x), 2, "{spawn}: cut at x = {x}");
            undo(&mut app);
        }
        run(&mut app, "(delete 0)");
    }
}

#[test]
fn a_tilted_thin_cut_severs_a_long_rotated_body() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box-rot 0 1 6 0.5 0.3)");
    for i in 0..10 {
        let x = 0.05 + i as f32 * 0.0031;
        let source = format!("(cut {} 3 {} -1 0.03)", x - 0.7, x + 0.7);
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(source);
        app.update();
        app.update();
        assert_eq!(bodies(&mut app).len(), 2, "cut at x = {x}");
        undo(&mut app);
    }
}

#[test]
fn a_cut_through_one_side_of_a_ring_opens_it() {
    // The stroke passes right through the ring's material; Algodoo's knife
    // opens the ring. The cut used to require two pieces, so it did nothing.
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 2 2) (spawn-box 0 1 1 1)");
    run(&mut app, "(subtract 0 1)");
    let before = app.world_mut().query::<&Body>().iter(app.world()).count();
    run(&mut app, "(cut 0 3 0 1 0.03)");
    let mut q = app.world_mut().query_filtered::<&ShapeDef, With<Body>>();
    let shapes: Vec<ShapeDef> = q.iter(app.world()).cloned().collect();
    assert_eq!(shapes.len(), before, "one piece replaces the ring");
    let holes: usize = shapes
        .iter()
        .map(|s| gradiance::geometry::polygonize::polygonize(s).holes.len())
        .sum();
    assert_eq!(holes, 0, "the ring is open: {shapes:?}");
}

#[test]
fn a_stroke_that_ends_inside_a_body_still_does_nothing() {
    let mut app = paused_app();
    run(&mut app, "(spawn-box 0 1 2 2) (spawn-box 0 1 1 1)");
    run(&mut app, "(subtract 0 1)");
    run(&mut app, "(spawn-box 4 1 1 1)");
    let before = bodies(&mut app);
    // Into the ring's side but not out of it; into the box's middle.
    for source in ["(cut 0 3 0 1.75 0.03)", "(cut 4 3 4 1 0.03)"] {
        app.world_mut()
            .resource_mut::<ScriptInputs>()
            .submit(source);
        app.update();
        app.update();
        assert_eq!(bodies(&mut app), before, "{source}");
    }
}

#[test]
fn a_moved_body_stays_dynamic_through_undo_and_redo() {
    // The move gesture holds the body kinematic; the move's undo step
    // recorded the hold, so a redo left the body frozen in the air.
    let mut app = paused_app();
    app.world_mut().insert_resource(CameraScale(APP_CAM_SCALE));
    run(&mut app, "(spawn-box 0 1 1 1)");
    use_tool(&mut app, ToolState::Select);
    drag(&mut app, Vec2::new(0.0, 1.0), Vec2::new(2.0, 1.5));
    assert_eq!(
        kind_list(&mut app),
        vec![RigidBody::Dynamic],
        "after the move"
    );
    undo(&mut app);
    crate::harness::redo(&mut app);
    assert_eq!(kind_list(&mut app), vec![RigidBody::Dynamic], "after redo");
    // And it falls when played.
    let y0 = bodies(&mut app)[0].1.y;
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Playing);
    for _ in 0..30 {
        app.update();
    }
    assert!(bodies(&mut app)[0].1.y < y0 - 0.1, "the body falls");
}

#[test]
fn a_move_while_playing_records_the_authored_kind() {
    let mut app = crate::harness::headless_app();
    app.world_mut().insert_resource(CameraScale(APP_CAM_SCALE));
    run(&mut app, "(spawn-ground 0 0 0) (spawn-box 0 0.5 1 1)");
    for _ in 0..30 {
        app.update();
    }
    use_tool(&mut app, ToolState::Select);
    let at = bodies(&mut app)[0].1;
    drag(&mut app, at, at + Vec2::new(1.0, 0.0));
    run(&mut app, "(spawn-box 5 0.5 1 1)");
    undo(&mut app);
    assert!(
        kind_list(&mut app).iter().all(|k| *k == RigidBody::Dynamic),
        "{:?}",
        kind_list(&mut app)
    );
}

fn kind_list(app: &mut App) -> Vec<RigidBody> {
    let mut q = app
        .world_mut()
        .query_filtered::<(&RigidBody, &ShapeDef), With<Body>>();
    q.iter(app.world())
        .filter(|(_, s)| !s.contains_half_plane())
        .map(|(k, _)| *k)
        .collect()
}
