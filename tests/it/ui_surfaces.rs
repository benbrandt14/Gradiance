//! F3 search, the tools pie and the status bar (brief U-9), end to end.
//!
//! Headless there is no egui pass, so each surface's leaf renderer is drawn
//! with `egui_kittest` over rows built from a headless app, driven by keys
//! and the pointer, and what it returns (an `InvokeOp`) is written into the
//! app as the egui system writes it (the `ui_groups` pattern).

use crate::harness::{box_record, entity_of, paused_app};
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy_egui::egui;
use egui_kittest::Harness;
use gradiance::command::CommandStack;
use gradiance::core::ids::StableId;
use gradiance::domain::Body;
use gradiance::domain::group::SelectionGroup;
use gradiance::interaction::selection::{SelectTransition, SelectedJoint, Selection};
use gradiance::op::{InvokeOp, OpContext, OpRegistry, invoke};
use gradiance::prelude::*;
use gradiance::ui::pie::{PieInput, PieMenu, PieSlice, TOOLS_PIE, pie_slices, pie_ui};
use gradiance::ui::search::{SearchPanel, SearchRow, search_rows, search_ui};
use serde_json::json;

/// A paused app with one box, selected.
fn app_with_a_selected_box() -> (App, StableId) {
    let mut app = paused_app();
    let record = box_record(Vec2::ZERO, 1.0, 0.5);
    let id = record.id;
    app.world_mut().write_message(SpawnBodyIntent { record });
    app.update();
    let picked = entity_of(&app, id).unwrap_or_else(|| panic!("the box spawned"));
    let result = app.world_mut().run_system_once(
        move |mut selection: ResMut<Selection>,
              mut joint: ResMut<SelectedJoint>,
              groups: Query<(Entity, &SelectionGroup), With<Body>>| {
            SelectTransition::SetBodies(vec![picked]).apply(&mut selection, &mut joint, &groups);
        },
    );
    assert!(result.is_ok(), "{result:?}");
    (app, id)
}

/// The rows the app's refresh system would build for `query`.
fn rows(app: &mut App, query: &str) -> Vec<SearchRow> {
    let entries = app.world().resource::<OpRegistry>().entries().to_vec();
    let ctx = OpContext::build(app.world_mut());
    search_rows(&entries, &ctx, query)
}

fn undo_len(app: &App) -> usize {
    app.world().resource::<CommandStack>().undo_len()
}

/// The search box's harness state.
#[derive(Default)]
struct SearchState {
    panel: SearchPanel,
    rows: Vec<SearchRow>,
    chosen: Vec<InvokeOp>,
}

/// F3 opens the box, typing ranks the rows, Enter invokes the top one
/// through `InvokeOp`, and the edit is one undo step.
#[test]
fn f3_search_types_and_enter_invokes_one_undo_step() {
    let (mut app, id) = app_with_a_selected_box();
    let before = undo_len(&app);
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut SearchState| {
            if let Some(invocation) = search_ui(ui.ctx(), &mut state.panel, &state.rows) {
                state.chosen.push(invocation);
            }
        },
        SearchState::default(),
    );
    harness.key_press(egui::Key::F3);
    harness.step();
    assert!(harness.state().panel.open, "F3 opens the box");
    harness.step();
    harness.event(egui::Event::Text("delete sel".to_owned()));
    harness.step();
    let query = harness.state().panel.query.clone();
    assert_eq!(query, "delete sel", "typing reaches the field");
    harness.state_mut().rows = rows(&mut app, &query);
    harness.step();
    let top = harness.state().rows.first().cloned();
    assert_eq!(top.as_ref().map(|r| r.id), Some("body.delete"), "{top:?}");
    assert_eq!(top.and_then(|r| r.disabled), None, "a box is selected");

    harness.key_press(egui::Key::Enter);
    harness.step();
    let chosen = std::mem::take(&mut harness.state_mut().chosen);
    assert_eq!(chosen.len(), 1, "Enter invokes once");
    assert!(!harness.state().panel.open, "invoking closes the box");
    assert_eq!(chosen[0].id, "body.delete");
    assert_eq!(chosen[0].params, json!({"i": "selection"}));

    app.world_mut().write_message(chosen[0].clone());
    app.update();
    app.update();
    assert!(entity_of(&app, id).is_none(), "the box is deleted");
    assert_eq!(undo_len(&app), before + 1, "one undo step");
}

/// A row that cannot run is greyed with its reason, and Enter on it does
/// nothing; Esc closes the box.
#[test]
fn a_disabled_row_says_why_and_enter_skips_it() {
    let mut app = paused_app();
    let all = rows(&mut app, "delete selection");
    let delete = all.iter().find(|r| r.id == "body.delete");
    let reason = delete.and_then(|r| r.disabled.clone());
    assert!(reason.is_some(), "nothing is selected: {delete:?}");

    let mut harness = Harness::new_ui_state(
        |ui, state: &mut SearchState| {
            if let Some(invocation) = search_ui(ui.ctx(), &mut state.panel, &state.rows) {
                state.chosen.push(invocation);
            }
        },
        SearchState {
            rows: all,
            ..SearchState::default()
        },
    );
    harness.key_press(egui::Key::F3);
    harness.step();
    harness.key_press(egui::Key::Enter);
    harness.step();
    assert!(
        harness.state().chosen.is_empty(),
        "a greyed row does not run"
    );
    assert!(harness.state().panel.open);
    harness.key_press(egui::Key::Escape);
    harness.step();
    assert!(!harness.state().panel.open, "Esc closes the box");
}

/// The search lists every operation a click can run, tools included, and
/// no query; `ui.search` answers the same ranking.
#[test]
fn search_lists_click_runnable_operations_and_ui_search_ranks_them() {
    let mut app = paused_app();
    let every = rows(&mut app, "");
    let ids: Vec<&str> = every.iter().map(|r| r.id).collect();
    for wanted in ["edit.undo", "body.delete", "group.create", "tool.box"] {
        assert!(ids.contains(&wanted), "{wanted} missing from {ids:?}");
    }
    for unwanted in ["body.count", "body.spawn_box", "ui.search"] {
        assert!(!ids.contains(&unwanted), "{unwanted} listed");
    }
    let ranked = invoke(app.world_mut(), "ui.search", json!({"text": "box"}));
    let first = ranked.ok().flatten().and_then(|v| v.get(0).cloned());
    assert_eq!(first, Some(json!("tool.box")));
}

/// The pie's harness state.
struct PieState {
    pie: PieMenu,
    slices: Vec<PieSlice>,
    chosen: Vec<InvokeOp>,
}

fn pie_harness(app: &App) -> Harness<'static, PieState> {
    let slices = pie_slices(app.world().resource::<OpRegistry>(), TOOLS_PIE);
    assert_eq!(slices.len(), TOOLS_PIE.len(), "every slice is registered");
    Harness::builder()
        .with_size(egui::vec2(800.0, 600.0))
        .build_ui_state(
            |ui, state: &mut PieState| {
                let input = PieInput {
                    canvas: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
                    over_ui: false,
                };
                if let Some(invocation) = pie_ui(ui.ctx(), &mut state.pie, &state.slices, input) {
                    state.chosen.push(invocation);
                }
            },
            PieState {
                pie: PieMenu::default(),
                slices,
                chosen: Vec::new(),
            },
        )
}

/// Where slice `i` of eight sits around the centre (400, 300).
fn toward(i: usize) -> egui::Pos2 {
    let angle = std::f32::consts::TAU * i as f32 / 8.0;
    egui::pos2(400.0 + 90.0 * angle.sin(), 300.0 - 90.0 * angle.cos())
}

fn tool(app: &App) -> ToolState {
    *app.world().resource::<State<ToolState>>().get()
}

/// Hold Q, move toward a slice, release: its tool operation runs.
#[test]
fn releasing_q_over_a_slice_switches_the_tool() {
    let mut app = paused_app();
    assert_eq!(tool(&app), ToolState::Select);
    let mut harness = pie_harness(&app);
    harness.hover_at(egui::pos2(400.0, 300.0));
    harness.step();
    harness.key_down(egui::Key::Q);
    harness.step();
    assert!(harness.state().pie.centre.is_some(), "Q opens the pie");
    // Slice 2 (east) is the circle tool.
    assert_eq!(TOOLS_PIE[2], "tool.circle");
    harness.hover_at(toward(2));
    harness.step();
    harness.key_up(egui::Key::Q);
    harness.step();
    let chosen = std::mem::take(&mut harness.state_mut().chosen);
    assert_eq!(chosen.len(), 1);
    assert_eq!(chosen[0].id, "tool.circle");
    assert!(harness.state().pie.centre.is_none(), "the pie closes");

    let before = undo_len(&app);
    app.world_mut().write_message(chosen[0].clone());
    app.update();
    app.update();
    assert_eq!(tool(&app), ToolState::Circle);
    assert_eq!(undo_len(&app), before, "a tool change is not an edit");
}

/// Hold, leave the centre and come back, release: nothing runs.
#[test]
fn releasing_q_back_in_the_centre_cancels() {
    let app = paused_app();
    let mut harness = pie_harness(&app);
    harness.hover_at(egui::pos2(400.0, 300.0));
    harness.step();
    harness.key_down(egui::Key::Q);
    harness.step();
    harness.hover_at(toward(4));
    harness.step();
    harness.hover_at(egui::pos2(402.0, 301.0));
    harness.step();
    harness.key_up(egui::Key::Q);
    harness.step();
    assert!(harness.state().chosen.is_empty());
    assert!(harness.state().pie.centre.is_none(), "the pie closed");
}

/// A tap of Q leaves the pie open; a click on a slice runs it.
#[test]
fn a_tap_keeps_the_pie_open_for_a_click() {
    let app = paused_app();
    let mut harness = pie_harness(&app);
    harness.hover_at(egui::pos2(400.0, 300.0));
    harness.step();
    harness.key_down(egui::Key::Q);
    harness.step();
    harness.key_up(egui::Key::Q);
    harness.step();
    assert!(harness.state().pie.centre.is_some(), "a tap keeps it open");
    let at = toward(1);
    harness.hover_at(at);
    harness.step();
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        harness.step();
    }
    let chosen: Vec<&str> = harness.state().chosen.iter().map(|i| &*i.id).collect();
    assert_eq!(chosen, [TOOLS_PIE[1]]);
}

/// The status bar is a panel: on by default, and `panel.hide` hides it.
#[test]
fn the_status_bar_is_a_panel_on_by_default() {
    let mut app = paused_app();
    let open = |app: &mut App| invoke(app.world_mut(), "panel.open", json!({"name": "status"}));
    assert_eq!(open(&mut app).ok().flatten(), Some(json!(true)));
    let hidden = invoke(app.world_mut(), "panel.hide", json!({"name": "status"}));
    assert!(hidden.is_ok(), "{hidden:?}");
    app.update();
    assert_eq!(open(&mut app).ok().flatten(), Some(json!(false)));
}
