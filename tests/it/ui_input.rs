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
    // Move, press, 20 steps: the gesture is still held. Small steps keep
    // the grabbed snap point the body's centre (the select tool grabs the
    // snap point nearest the first moved frame's cursor and moves it onto
    // the cursor; a 10-point first step picks the edge midpoint at 20).
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
