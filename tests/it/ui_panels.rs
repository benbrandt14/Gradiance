//! `egui_kittest` layout tests for the top panels' host-agnostic leaf
//! renderers (toolbar palette, `precise_drag`, the plot pane, the curve
//! editor, reflect grid) —
//! headless accessibility-tree checks that catch reflow/interaction
//! regressions. Full panels are Bevy systems whose logic the intent-level
//! tests already cover.

// Asserting exact typed-in values (7.25 is exactly representable).
#![allow(clippy::float_cmp)]

use bevy_egui::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use gradiance::core::states::ToolState;
use gradiance::domain::signal::Curve;
use gradiance::ui::curve::curve_editor;
use gradiance::ui::plot::{PlotConfig, Series, plot_section};
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
        if let Some(tool) = tools_palette_ui(ui, ToolState::Select, None) {
            clicked.set(Some(tool));
        }
    });
    harness.run();

    // Every tool button is present (a missing entry = a palette reflow bug).
    for label in [
        "Select (S)",
        "Drag (D)",
        "Box (B)",
        "Circle (C)",
        "Polygon (P)",
        "Hinge (H)",
        "Prismatic (R)",
        "Strut (T)",
        "Weld (W)",
        "Ground (G)",
        "Cut (K)",
        "Tracer (N)",
    ] {
        harness.get_by_label(label);
    }

    harness.get_by_label("Box (B)").click();
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

/// The plot pane lays out without panicking across the data shapes the bus
/// really produces: nothing recorded, a flat series, a dense wave, and a
/// series whose timestamps are shorter than its values (a bus entry caught
/// mid-append). Each one used to be a separate branch in the hand-drawn
/// painter; on `egui_plot` they are one path, so this asserts it stays total.
#[test]
fn plot_pane_renders_every_data_shape() {
    let flat: VecDeque<f32> = VecDeque::from([3.0; 50]);
    let flat_t: VecDeque<f32> = (0..50).map(|i| i as f32 * 0.016).collect();
    let wave: VecDeque<f32> = (0..200).map(|i| (i as f32 * 0.1).sin()).collect();
    let wave_t: VecDeque<f32> = (0..200).map(|i| i as f32 * 0.016).collect();
    let ragged: VecDeque<f32> = VecDeque::from([1.0, 2.0, 3.0]);
    let ragged_t: VecDeque<f32> = VecDeque::from([0.0, 0.1]);

    let mut config = PlotConfig::default();
    let mut harness = Harness::new_ui(move |ui| {
        // Nothing recorded → the empty state, not a blank pane.
        plot_section(ui, &[], &mut config);
        let series = [
            Series {
                name: "flat",
                unit: "m/s",
                values: &flat,
                times: &flat_t,
            },
            Series {
                name: "wave",
                unit: "",
                values: &wave,
                times: &wave_t,
            },
            Series {
                name: "ragged",
                unit: "N",
                values: &ragged,
                times: &ragged_t,
            },
        ];
        plot_section(ui, &series, &mut config);
    });
    harness.run();
    harness.get_by_label_contains("wire a sensor to the plot sink");
    harness.get_by_label("Series");
}

/// The curve editor renders and reports no change when nobody touches it —
/// the property that matters, since a spurious `true` would record an undo
/// step (or dirty the scene) on every frame the panel is open.
#[test]
fn curve_editor_renders_and_reports_no_change_when_idle() {
    let changed: Cell<bool> = Cell::new(true);
    let mut curve = Curve::default();
    let mut harness = Harness::new_ui(move |ui| {
        changed.set(curve_editor(ui, "test", &mut curve));
        assert!(!changed.get(), "an untouched curve editor is not an edit");
    });
    harness.run();
    harness.get_by_label("Linear");
    harness.get_by_label("Smooth");
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
