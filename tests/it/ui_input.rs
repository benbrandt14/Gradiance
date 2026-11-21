//! Input injection: `ui.click`, `ui.drag`, `ui.key`, `ui.text`, `ui.wheel`
//! (brief U-4).
//!
//! Headless there is no egui pass and no camera, so these tests check what
//! the injection writes and what the layers below egui make of it: the
//! window's cursor, `ButtonInput` edges, the editor's key handler and the
//! select tool's gesture. The window is a bare `Window` entity, and a
//! stand-in system maps its cursor to the world as the default 2D view does
//! (world `(0, 0)` at window `(640, 370)`, one world unit per point, y up).
//! What egui makes of the same messages (a palette click selecting a tool,
//! text in a field) is checked live under Xvfb (the brief's Outcome).

use crate::harness::{box_record, entity_of, paused_app};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent, WindowResolution};
use gradiance::command::CommandStack;
use gradiance::interaction::cursor::{CursorWorldPos, update_cursor_world_pos};
use gradiance::interaction::snap::update_snapped_cursor;
use gradiance::op::{OpRegistry, invoke};
use gradiance::prelude::*;
use gradiance::ui::host::input::parse_chord;
use gradiance::ui::widget_registry::{clear_handles, end_frame, publish_target};
use serde_json::{Value, json};

/// Window point to world, as the default 2D view maps it.
fn world_of(screen: Vec2) -> Vec2 {
    Vec2::new(screen.x - 640.0, 370.0 - screen.y)
}

/// Stands in for the camera: the window's cursor to the sandbox plane.
fn stand_in_camera(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cursor: ResMut<CursorWorldPos>,
) {
    cursor.0 = windows
        .iter()
        .next()
        .and_then(Window::cursor_position)
        .map(world_of);
}

/// What one frame's input looked like after `InputSystems`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Seen {
    cursor: Option<Vec2>,
    pressed: bool,
    just_pressed: bool,
    just_released: bool,
    /// The middle button went down or up this frame.
    middle_edge: bool,
    window_events: usize,
}

#[derive(Resource, Default)]
struct SeenLog(Vec<Seen>);

fn record(
    windows: Query<&Window, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut events: MessageReader<WindowEvent>,
    mut log: ResMut<SeenLog>,
) {
    let left = MouseButton::Left;
    log.0.push(Seen {
        cursor: windows.iter().next().and_then(Window::cursor_position),
        pressed: buttons.pressed(left),
        just_pressed: buttons.just_pressed(left),
        just_released: buttons.just_released(left),
        middle_edge: buttons.just_pressed(MouseButton::Middle)
            || buttons.just_released(MouseButton::Middle),
        window_events: events.read().count(),
    });
}

/// A paused app with a 1280x720 primary window, the stand-in camera and
/// the input recorder.
fn windowed_app() -> App {
    let mut app = paused_app();
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(1280, 720),
            ..default()
        },
        PrimaryWindow,
    ));
    app.init_resource::<SeenLog>();
    app.add_systems(
        PreUpdate,
        (
            stand_in_camera
                .after(update_cursor_world_pos)
                .before(update_snapped_cursor),
            record.after(InputSystems),
        ),
    );
    app.update();
    app.world_mut().resource_mut::<SeenLog>().0.clear();
    app
}

fn op(app: &mut App, id: &str, params: Value) -> Result<Option<Value>, String> {
    invoke(app.world_mut(), id, params).map_err(|e| e.to_string())
}

fn undo_len(app: &App) -> usize {
    app.world().resource::<CommandStack>().undo_len()
}

/// A click on a widget name resolves through the widget mirror to the
/// widget's handle and plays as move, press, release, one frame each, so
/// `ButtonInput` sees a real press edge at the widget (and egui, reading the
/// same `WindowEvent`s, a click).
#[test]
fn a_click_by_name_plays_move_press_release_at_the_widget() {
    let mut app = windowed_app();
    clear_handles();
    publish_target(
        "tools:Box",
        bevy_egui::egui::Rect::from_min_max(
            bevy_egui::egui::pos2(10.0, 130.0),
            bevy_egui::egui::pos2(40.0, 160.0),
        ),
    );
    app.world_mut()
        .run_system_cached(end_frame)
        .unwrap_or_else(|e| panic!("end_frame: {e}"));

    assert_eq!(
        op(&mut app, "ui.click", json!({"at": "tools:Box"})),
        Ok(None)
    );
    for _ in 0..4 {
        app.update();
    }
    let log = app.world().resource::<SeenLog>().0.clone();
    let at = Some(Vec2::new(25.0, 145.0));
    assert_eq!(log[0].cursor, at, "the first frame moves to the handle");
    assert!(!log[0].pressed, "and does not press yet");
    assert!(log[1].just_pressed && log[1].cursor == at, "{log:?}");
    assert!(log[2].just_released, "{log:?}");
    assert_eq!(
        log.iter().map(|s| s.window_events).collect::<Vec<_>>()[..3],
        [1, 1, 1],
        "each frame also writes the WindowEvent egui reads"
    );
    assert_eq!(log[3].window_events, 0, "then nothing");

    let unknown = op(&mut app, "ui.click", json!({"at": "tools:Nope"}));
    assert!(
        unknown
            .as_ref()
            .is_err_and(|e| e.contains("no widget `tools:Nope`")),
        "{unknown:?}"
    );
}

/// `ui.click_button` names the button (brief U-8b: a middle click resets a
/// number field): the same move, press, release, on the middle button only.
#[test]
fn a_named_button_click_presses_that_button() {
    let mut app = windowed_app();
    assert_eq!(
        op(
            &mut app,
            "ui.click_button",
            json!({"at": [25.0, 145.0], "button": "middle"})
        ),
        Ok(None)
    );
    for _ in 0..4 {
        app.update();
    }
    let log = app.world().resource::<SeenLog>().0.clone();
    assert_eq!(log[0].cursor, Some(Vec2::new(25.0, 145.0)));
    assert!(log[1].middle_edge && log[2].middle_edge, "{log:?}");
    assert!(!log[3].middle_edge, "{log:?}");
    assert!(
        log.iter().all(|s| !s.pressed && !s.just_released),
        "the left button stays up: {log:?}"
    );
    let bad = op(
        &mut app,
        "ui.click_button",
        json!({"at": [1.0, 1.0], "button": "side"}),
    );
    assert!(
        bad.as_ref()
            .is_err_and(|e| e.contains("not left, middle or right")),
        "{bad:?}"
    );
}

/// An injected `Ctrl+Z` reaches the editor's key handler like a real one:
/// the spawn is undone. And `Delete` deletes the selection.
#[test]
fn an_injected_chord_reaches_the_editor() {
    let mut app = windowed_app();
    let record = box_record(Vec2::ZERO, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    assert!(entity_of(&app, id).is_some());
    let before = undo_len(&app);

    op(&mut app, "ui.key", json!({"key": "Ctrl+Z"})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(undo_len(&app), before - 1, "Ctrl+Z undid one step");
    assert!(entity_of(&app, id).is_none(), "the spawn is undone");

    let bad = op(&mut app, "ui.key", json!({"key": "Hyper+Q"}));
    assert!(bad.is_err(), "{bad:?}");
}

/// A drag on a body, injected over many frames, selects it on press, moves
/// it while held without touching history, and commits exactly one undo
/// step on release (invariant 2 holds for injected input).
#[test]
fn an_injected_drag_on_a_body_is_one_undo_step() {
    let mut app = windowed_app();
    let record = box_record(Vec2::ZERO, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    app.update(); // colliders
    let before = undo_len(&app);

    op(
        &mut app,
        "ui.drag",
        json!({"from": [640, 370], "to": [740, 370], "steps": 20}),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // Move, press, 20 steps: the gesture is still held. The select tool
    // grabs the snap point nearest the press (here the body's centre) and
    // moves it onto the cursor.
    for _ in 0..22 {
        app.update();
        assert_eq!(undo_len(&app), before, "nothing commits mid-drag");
    }
    let entity = entity_of(&app, id).unwrap_or_else(|| panic!("body gone"));
    assert!(
        app.world().resource::<Selection>().contains(entity),
        "the press selected the body"
    );
    // The release, and a frame for the command.
    app.update();
    app.update();
    assert_eq!(undo_len(&app), before + 1, "one gesture, one undo step");
    let entity = entity_of(&app, id).unwrap_or_else(|| panic!("body gone"));
    let pos = app
        .world()
        .get::<Transform>(entity)
        .map(|t| t.translation.truncate());
    assert!(
        pos.is_some_and(|p| (p - Vec2::new(100.0, 0.0)).length() < 1.0),
        "{pos:?}"
    );

    let refused = op(
        &mut app,
        "ui.drag",
        json!({"from": [0, 0], "to": [1, 1], "steps": 0}),
    );
    assert!(refused.is_err(), "steps 0: {refused:?}");
}

/// A fast drag (a long first step) with the box tool spans the box from the
/// **press** point to the release point. The tools' button edges must be read
/// after `InputSystems`, in the frame the press arrives; read before it, the
/// press shows a frame late, when the cursor has already taken its first step
/// (brief A-1: every drag tool started one step late, and a fast select-tool
/// drag missed the body it was pressed on).
#[test]
fn a_fast_box_drag_spans_from_the_press_point() {
    let mut app = windowed_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Box);
    app.update();

    op(
        &mut app,
        "ui.drag",
        json!({"from": [600, 330], "to": [700, 430], "steps": 4}),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..10 {
        app.update();
    }
    let mut q = app
        .world_mut()
        .query_filtered::<(&ShapeDef, &Transform), With<Body>>();
    let spawned: Vec<(ShapeDef, Vec2)> = q
        .iter(app.world())
        .map(|(s, t)| (s.clone(), t.translation.truncate()))
        .collect();
    assert_eq!(spawned.len(), 1, "{spawned:?}");
    let (shape, centre) = &spawned[0];
    assert_eq!(
        *shape,
        ShapeDef::Box {
            width: 100.0,
            height: 100.0
        },
        "press (600, 330) to release (700, 430) is 100 x 100"
    );
    assert!(
        (*centre - Vec2::new(10.0, -10.0)).length() < 1e-3,
        "{centre:?}"
    );
}

/// A fast select-tool drag pressed on a body moves that body (it used to
/// start a band selection at the first step's point, off the body).
#[test]
fn a_fast_select_drag_moves_the_pressed_body() {
    let mut app = windowed_app();
    let record = box_record(Vec2::ZERO, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    app.update(); // colliders
    let before = undo_len(&app);

    op(
        &mut app,
        "ui.drag",
        json!({"from": [640, 370], "to": [640, 570], "steps": 2}),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..8 {
        app.update();
    }
    assert_eq!(undo_len(&app), before + 1, "one move, one undo step");
    let entity = entity_of(&app, id).unwrap_or_else(|| panic!("body gone"));
    let pos = app
        .world()
        .get::<Transform>(entity)
        .map(|t| t.translation.truncate());
    assert!(
        pos.is_some_and(|p| (p - Vec2::new(0.0, -200.0)).length() < 1.0),
        "the grabbed centre follows the cursor: {pos:?}"
    );
}

/// `ui.key` reads every chord the registry declares (the text menus show).
#[test]
fn every_declared_chord_parses() {
    let app = paused_app();
    let registry = app.world().resource::<OpRegistry>();
    let chords: Vec<&str> = registry
        .entries()
        .iter()
        .filter_map(|e| e.meta.chord)
        .collect();
    assert_ne!(chords, Vec::<&str>::new(), "the registry declares chords");
    for chord in chords {
        assert!(parse_chord(chord).is_ok(), "{chord}");
    }
}

/// Text and wheel turns queue; a non-finite point or turn is refused before
/// anything is queued.
#[test]
fn bad_points_are_refused_and_text_and_wheel_queue() {
    let mut app = windowed_app();
    assert_eq!(op(&mut app, "ui.text", json!({"text": "1.5\n"})), Ok(None));
    assert_eq!(
        op(&mut app, "ui.wheel", json!({"at": [640, 370], "lines": -2})),
        Ok(None)
    );
    for _ in 0..3 {
        app.update();
    }
    let log = app.world().resource::<SeenLog>().0.clone();
    // Text: 4 characters, each a press and a release, in one frame; then
    // the wheel's move and turn.
    assert_eq!(
        log.iter().map(|s| s.window_events).collect::<Vec<_>>(),
        [8, 1, 1]
    );
    let bad = op(
        &mut app,
        "ui.wheel",
        json!({"at": [640, 370], "lines": f32::NAN}),
    );
    assert!(bad.is_err(), "{bad:?}");
}

/// A-3: Ctrl+Z with the pointer resting on a number field undoes. The
/// field still owns its bare keys: Backspace over it deletes nothing.
#[test]
fn ctrl_z_over_a_hovered_number_field_undoes() {
    use gradiance::interaction::selection::Selection;
    use gradiance::interaction::{FieldKeys, KeyboardCaptured};
    let mut app = windowed_app();
    let keep = box_record(Vec2::new(-200.0, 0.0), 40.0, 20.0);
    let keep_id = keep.id;
    app.world_mut()
        .write_message(SpawnBodyIntent { record: keep });
    app.update();
    let record = box_record(Vec2::ZERO, 40.0, 20.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    let before = undo_len(&app);
    // What the UI layer writes while the pointer is on a field (no focus):
    // headless there is no egui pass to overwrite it.
    app.insert_resource(KeyboardCaptured(true));
    app.insert_resource(FieldKeys(true));

    let keep_entity = entity_of(&app, keep_id).unwrap_or_else(|| panic!("no body"));
    app.world_mut().resource_mut::<Selection>().set(keep_entity);
    op(&mut app, "ui.key", json!({"key": "Backspace"})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert!(
        entity_of(&app, keep_id).is_some(),
        "Backspace is the field's"
    );
    assert_eq!(undo_len(&app), before);

    op(&mut app, "ui.key", json!({"key": "Ctrl+Z"})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(
        undo_len(&app),
        before - 1,
        "Ctrl+Z over a field undid one step"
    );
    assert!(entity_of(&app, id).is_none(), "the spawn is undone");

    // A text edit with focus keeps every key, Ctrl chords included.
    app.insert_resource(FieldKeys(false));
    op(&mut app, "ui.key", json!({"key": "Ctrl+Z"})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(undo_len(&app), before - 1, "typing swallows Ctrl+Z");
}

/// Welds two overlapping boxes with one click of the weld tool.
fn weld_click(app: &mut App) {
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Weld);
    app.update();
    op(app, "ui.click", json!({"at": [640, 370]})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..6 {
        app.update();
    }
}

/// A-3: a click a tool used is not also a selection click. A weld commits
/// a merge (not one of the three intents the click-through looked for), so
/// the welded body was selected; a polygon closed by clicking its first
/// vertex commits on the press, so the release selected the new polygon.
#[test]
fn a_click_that_a_tool_commits_does_not_select() {
    use gradiance::interaction::selection::Selection;
    let mut app = windowed_app();
    for x in [-15.0, 15.0] {
        let record = box_record(Vec2::new(x, 0.0), 40.0, 20.0);
        app.world_mut().write_message(SpawnBodyIntent { record });
        app.update();
    }
    app.update(); // colliders
    weld_click(&mut app);
    let mut bodies = app.world_mut().query_filtered::<Entity, With<Body>>();
    assert_eq!(bodies.iter(app.world()).count(), 1, "the weld merged them");
    assert!(
        app.world().resource::<Selection>().is_empty(),
        "the weld click selected the welded body"
    );

    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Polygon);
    app.update();
    for p in [[700, 300], [800, 300], [800, 200], [700, 300]] {
        op(&mut app, "ui.click", json!({"at": p})).unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..4 {
            app.update();
        }
    }
    assert_eq!(bodies.iter(app.world()).count(), 2, "the polygon closed");
    assert!(
        app.world().resource::<Selection>().is_empty(),
        "the closing click selected the new polygon"
    );
}

/// A-3: with the Tracer tool active, a click on the UI (a menu, the
/// palette) placed a free tracer where the pointer was: the driver reports
/// every release, and the tracer placed on any release. A click on the
/// canvas still places one.
#[test]
fn a_tracer_is_placed_only_by_a_click_on_the_canvas() {
    use gradiance::interaction::PointerOverUi;
    let mut app = windowed_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Tracer);
    app.update();
    let nodes = |app: &mut App| {
        app.world_mut()
            .query::<&gradiance::domain::node::NodeKind>()
            .iter(app.world())
            .count()
    };

    // What the UI layer writes while the pointer is on a menu.
    app.insert_resource(PointerOverUi(true));
    op(&mut app, "ui.click", json!({"at": [53, 11]})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(nodes(&mut app), 0, "a click on the UI placed a tracer");

    app.insert_resource(PointerOverUi(false));
    op(&mut app, "ui.click", json!({"at": [640, 370]})).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..5 {
        app.update();
    }
    assert_eq!(nodes(&mut app), 1, "a canvas click places one tracer");
}

/// Two lasers drawn one after the other with the Laser tool take two bus
/// names, `laser` then `laser2`, so they publish two values (brief Q-4).
#[test]
fn two_drawn_lasers_take_two_names() {
    use gradiance::domain::node::NodeKind;
    let mut app = windowed_app();
    app.world_mut()
        .resource_mut::<NextState<ToolState>>()
        .set(ToolState::Laser);
    app.update();
    for y in [300, 400] {
        op(
            &mut app,
            "ui.drag",
            json!({"from": [600, y], "to": [700, y], "steps": 4}),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        for _ in 0..10 {
            app.update();
        }
    }
    let mut q = app.world_mut().query::<&NodeKind>();
    let mut names: Vec<String> = q
        .iter(app.world())
        .filter_map(|kind| match kind {
            NodeKind::Laser(laser) => Some(laser.name.clone()),
            _ => None,
        })
        .collect();
    names.sort();
    assert_eq!(names, ["laser", "laser2"]);
}
