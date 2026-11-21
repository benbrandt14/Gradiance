//! The context menu's open contract: a right *click* opens it, a right
//! *drag* past the shared deadzone does not, and a right-click while the
//! left button is held (a gesture owns the pointer) never opens it.
//!
//! The menu's first-frame survival — the opening click must not count as
//! "clicked elsewhere" while egui is still sizing the new area — lives in
//! the render pass, so it is verified live (the PR's screenshots); what is
//! pinned here is everything below the pass: the open logic, the deadzone
//! and the left-held suppression, driven exactly as a hand drives them
//! (`ui.click_button`, `ui.drag` with a named button, or raw button
//! state).

use crate::harness::{box_record, paused_app};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, Window, WindowResolution};
use gradiance::command::intent::SpawnBodyIntent;
use gradiance::interaction::cursor::{CursorWorldPos, update_cursor_world_pos};
use gradiance::interaction::pointer::RIGHT_CLICK_DEADZONE_PX;
use gradiance::interaction::snap::update_snapped_cursor;
use gradiance::op::invoke;
use gradiance::ui::context_menu::ContextMenu;
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

/// A paused app with a 1280x720 primary window and the stand-in camera.
fn windowed_app() -> App {
    let mut app = paused_app();
    app.world_mut().spawn((
        Window {
            resolution: WindowResolution::new(1280, 720),
            ..default()
        },
        PrimaryWindow,
    ));
    app.add_systems(
        PreUpdate,
        stand_in_camera
            .after(update_cursor_world_pos)
            .before(update_snapped_cursor),
    );
    app.update();
    app
}

fn op(app: &mut App, id: &str, params: Value) {
    invoke(app.world_mut(), id, params).unwrap_or_else(|e| panic!("{id}: {e}"));
}

fn menu_of(app: &App) -> ContextMenu {
    app.world().resource::<ContextMenu>().clone()
}

fn click_right(app: &mut App, at: [f32; 2]) {
    op(app, "ui.click_button", json!({"at": at, "button": "right"}));
    for _ in 0..4 {
        app.update();
    }
}

/// A right-click on the canvas opens the menu anchored at the click, and
/// a second right-click re-anchors it at the new click.
#[test]
fn a_right_click_opens_the_menu_at_the_click() {
    let mut app = windowed_app();
    click_right(&mut app, [640.0, 220.0]);
    let menu = menu_of(&app);
    assert!(menu.open, "the menu opened: {menu:?}");
    assert_eq!(menu.screen, Vec2::new(640.0, 220.0));
    assert_eq!(menu.under, Vec::new(), "empty canvas: no body offered");
    assert!(menu.joint.is_none() && menu.node.is_none());
    assert!(menu.fresh, "the first render pass must see a fresh menu");

    click_right(&mut app, [200.0, 150.0]);
    let menu = menu_of(&app);
    assert!(menu.open, "the second click keeps it open: {menu:?}");
    assert_eq!(menu.screen, Vec2::new(200.0, 150.0), "re-anchored");
}

/// A right-click over a body offers that body (topmost first).
#[test]
fn a_right_click_over_a_body_offers_it() {
    let mut app = windowed_app();
    let record = box_record(Vec2::ZERO, 160.0, 160.0);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();

    click_right(&mut app, [640.0, 370.0]);
    let menu = menu_of(&app);
    assert!(menu.open);
    assert_eq!(menu.under, vec![id], "the body under the click: {menu:?}");
}

/// A right-drag past the shared deadzone is a gesture (rotate, pan), not
/// a click: no menu. A drag that stays inside the deadzone still clicks.
#[test]
fn a_right_drag_past_the_deadzone_is_not_a_click() {
    let mut app = windowed_app();
    let deadzone = RIGHT_CLICK_DEADZONE_PX;

    // Inside the deadzone: the release still counts as the menu's click.
    let stay = 640.0 + deadzone - 1.0;
    op(
        &mut app,
        "ui.drag",
        json!({"from": [640.0, 220.0], "to": [stay, 220.0], "steps": 2, "button": "right"}),
    );
    for _ in 0..8 {
        app.update();
    }
    let menu = menu_of(&app);
    assert!(
        menu.open,
        "a release one px inside the deadzone still opens: {menu:?}"
    );
    assert_eq!(menu.screen.x, stay, "anchored at the release point");

    // The open menu is the render pass's to close; take it out of the way
    // so the second half asserts what this system alone does.
    app.world_mut()
        .get_resource_mut::<ContextMenu>()
        .expect("the menu resource")
        .open = false;
    app.update();

    // Past it: never a click, whichever gesture owns the drag.
    let far = 640.0 + deadzone + 2.0;
    op(
        &mut app,
        "ui.drag",
        json!({"from": [640.0, 220.0], "to": [far, 220.0], "steps": 3, "button": "right"}),
    );
    for _ in 0..8 {
        app.update();
    }
    assert!(
        !menu_of(&app).open,
        "a drag past the deadzone opens nothing"
    );
}

/// A right-click while the left button is held (a drag or a draft owns the
/// pointer) never opens the menu: it must not appear over a running
/// gesture, and its click-through selection must not hijack the gesture's.
#[test]
fn a_right_click_while_the_left_button_is_held_opens_nothing() {
    let mut app = windowed_app();

    // Hold the left button down as a gesture in flight would.
    app.world_mut()
        .resource_mut::<ButtonInput<MouseButton>>()
        .press(MouseButton::Left);
    app.update();

    click_right(&mut app, [640.0, 220.0]);
    let menu = menu_of(&app);
    assert!(!menu.open, "the menu stayed closed: {menu:?}");
    assert_eq!(menu.under, Vec::new(), "and picked nothing");
}
