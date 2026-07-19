//! `egui_kittest` layout tests for the top panels' host-agnostic leaf
//! renderers (toolbar palette, `precise_drag`, plot series, reflect grid) —
//! headless accessibility-tree checks that catch reflow/interaction
//! regressions. Full panels are Bevy systems whose logic the intent-level
//! tests already cover.

// Asserting exact typed-in values (7.25 is exactly representable).
#![allow(clippy::float_cmp)]

use bevy_egui::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gradiance::core::states::ToolState;
use gradiance::ui::plot::draw_series;
use gradiance::ui::reflect_grid::reflect_grid;
use gradiance::ui::toolbar::tools_palette_ui;
use gradiance::ui::widgets::{Commit, precise_drag};
use std::cell::Cell;
use std::collections::VecDeque;

/// The toolbar palette renders every tool and reports a click.
#[test]
fn toolbar_palette_lists_all_tools_and_reports_clicks() {
    let clicked: Cell<Option<ToolState>> = Cell::new(None);
    let mut harness = Harness::new_ui(|ui| {
        if let Some(tool) = tools_palette_ui(ui, ToolState::Select) {
            clicked.set(Some(tool));
        }
    });
    harness.run();

    // Every tool icon is present (a missing entry = a palette reflow bug).
    for icon in ["▧", "✋", "▭", "⬤", "⬠", "⊙", "⬌", "∿", "⧉", "⏚", "✂", "⌇"]
    {
        harness.get_by_label(icon);
    }

    harness.get_by_label("▭").click(); // Box
    harness.run();
    assert_eq!(clicked.get(), Some(ToolState::Box));
}

/// The inspector's committing drag widget renders and commits an edit once.
#[test]
fn precise_drag_renders_and_commits_typed_input() {
    let value = Cell::new(2.5_f32);
    let committed: Cell<Option<(f32, f32)>> = Cell::new(None);
    let mut harness = Harness::new_ui(|ui| {
        let mut v = value.get();
        if let Commit::Done(old, new) = precise_drag(ui, egui::Id::new("t"), &mut v, 1.0, 0.01) {
            committed.set(Some((old, new)));
        }
        value.set(v);
        // Focus target: moving focus here defocuses the drag, which is the
        // widget's commit point.
        let _ = ui.button("elsewhere");
    });
    harness.run();

    // The DragValue is present and focusable; type a new value, then move
    // focus away (commit-on-release/focus-loss).
    harness
        .get_by_role(egui::accesskit::Role::SpinButton)
        .focus();
    harness.run();
    harness
        .get_by_role(egui::accesskit::Role::SpinButton)
        .type_text("7.25");
    harness.run();
    harness.get_by_label("elsewhere").focus();
    harness.run();
    harness.run();

    let (old, new) = committed.get().expect("edit committed once on defocus");
    assert_eq!(old, 2.5);
    assert_eq!(new, 7.25);
    assert_eq!(value.get(), 7.25);
}

/// The plot's series renderer lays out a label + canvas without panicking on
/// empty, short, and flat data (the auto-scale edge cases).
#[test]
fn plot_series_renders_all_data_shapes() {
    let cases: Vec<VecDeque<f32>> = vec![
        VecDeque::new(),
        VecDeque::from([1.0]),
        VecDeque::from([3.0; 50]),                    // flat: hi == lo
        (0..200).map(|i| (i as f32).sin()).collect(), // dense wave
    ];
    let mut harness = Harness::new_ui(move |ui| {
        for (i, data) in cases.iter().enumerate() {
            draw_series(ui, &format!("signal {i}"), data, egui::Color32::LIGHT_BLUE);
        }
    });
    harness.run();
    for i in 0..4 {
        harness.get_by_label(&format!("signal {i}"));
    }
}

/// The settings panel's reflect grid renders a row per field and edits
/// write through to the reflected struct.
#[test]
fn reflect_grid_renders_settings_fields() {
    let mut settings = gradiance::domain::settings::SimSettings::default();
    let mut harness = Harness::new_ui(move |ui| {
        reflect_grid(ui, egui::Id::new("sim"), &mut settings);
    });
    harness.run();
    // One label per SimSettings field (underscores become spaces).
    harness.get_by_label("gravity");
    harness.get_by_label("speed");
    harness.get_by_label("substeps");
}

/// The Lighting tab's reflect half (the Backdrop grid over
/// `ScenerySettings`) renders every field row; the light list, sun
/// gadgets, and color pickers are hand-drawn and covered by the pure
/// view-cube/settings unit tests.
#[test]
fn lighting_and_scenery_settings_render_rows() {
    let mut scenery = gradiance::domain::settings::ScenerySettings::default();
    let mut harness = Harness::new_ui(move |ui| {
        reflect_grid(ui, egui::Id::new("scenery"), &mut scenery);
    });
    harness.run();
    for label in [
        "back offset",
        "back visible",
        "ground visible",
        "perspective deg",
    ] {
        harness.get_by_label(label);
    }
}
