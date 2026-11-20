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
        if let Some(tool) = tools_palette_ui(ui, ToolState::Select, None, &[]) {
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
        "Spring (T)",
        "Rod (L)",
        "Weld (W)",
        "Ground (G)",
        "Cut (K)",
        "Tracer (N)",
        "Thruster",
        "Chain",
    ] {
        harness.get_by_label(label);
    }

    harness.get_by_label("Box (B)").click();
    harness.run();
    assert_eq!(clicked.get(), Some(ToolState::Box));
}

/// A script-registered tool appears after the built-ins and selects its state.
#[test]
fn toolbar_palette_lists_script_tools_after_the_builtins() {
    let tools = [gradiance::core::script_tools::ScriptToolSpec {
        name: "ramp".into(),
        points: 2,
        source: String::new(),
    }];
    let clicked: Cell<Option<ToolState>> = Cell::new(None);
    let mut harness = Harness::new_ui(|ui| {
        if let Some(tool) = tools_palette_ui(ui, ToolState::Select, None, &tools) {
            clicked.set(Some(tool));
        }
    });
    harness.run();
    harness.get_by_label("ramp").click();
    harness.run();
    assert_eq!(clicked.get(), Some(ToolState::Script(0)));
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
                unit: gradiance::units::DisplayUnit::canonical(
                    gradiance::units::Dimension::Velocity,
                ),
                values: &flat,
                times: &flat_t,
            },
            Series {
                name: "wave",
                unit: gradiance::units::DisplayUnit::canonical(
                    gradiance::units::Dimension::Dimensionless,
                ),
                values: &wave,
                times: &wave_t,
            },
            Series {
                name: "ragged",
                unit: gradiance::units::DisplayUnit::canonical(gradiance::units::Dimension::Force),
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

/// Two groups, the second nested in the first and named, plus a loose box:
/// the Groups panel's fixture (also its snapshot's).
pub fn groups_fixture() -> gradiance::ui::groups::GroupsModel {
    use gradiance::core::ids::StableId;
    use gradiance::domain::appearance::PALETTE;
    use gradiance::domain::group::GroupNames;
    use gradiance::ui::groups::{BodyFacts, GroupsModel};
    // Ids in a known order, so each row's swatch (its first member's fill)
    // is the same every run.
    let mut ids: Vec<StableId> = (0..4).map(|_| StableId::new()).collect();
    ids.sort_by_key(|id| id.0);
    let mut ids = ids.into_iter();
    let mut body = |groups: &[u32], slot: usize, kind: &'static str| BodyFacts {
        id: ids.next().unwrap_or_default(),
        groups: groups.to_vec(),
        fill: PALETTE[slot].color,
        kind,
    };
    let mut names = GroupNames::default();
    names.0.insert(2, "wheels".into());
    GroupsModel::build(
        vec![
            body(&[2, 1], 0, "circle"),
            body(&[2, 1], 0, "circle"),
            body(&[1], 7, "box"),
            body(&[], 4, "box"),
        ],
        &names,
        2,
    )
}

/// The Groups panel: rows, the property picker, the colour swatches, the
/// distribute and split buttons and the filter chips each record the
/// operation (or selection) they stand for, with the group's index.
#[test]
fn groups_panel_rows_picker_and_chips_emit_their_operations() {
    use gradiance::ui::groups::{GroupsFilter, GroupsOutput, GroupsPanel, groups_section};
    use std::cell::RefCell;
    let model = groups_fixture();
    let panel = RefCell::new(GroupsPanel::default());
    let out = RefCell::new(GroupsOutput::default());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(320.0, 700.0))
        .build_ui(|ui| {
            groups_section(ui, &model, &mut panel.borrow_mut(), &mut out.borrow_mut());
        });
    harness.run();
    // Rows in tree order: the outer group, then the named inner one.
    harness.get_by_label("Group 0");
    harness.get_by_label("wheels").click();
    harness.run();
    let taken = std::mem::take(&mut *out.borrow_mut());
    assert_eq!(
        taken.filter,
        Some(GroupsFilter {
            clauses: vec!["group:1".into()]
        }),
        "a row click selects its group by index"
    );
    assert_eq!(panel.borrow().active(), Some(2));

    harness.get_by_label("colour blue").click();
    harness.run();
    // Pick `density` in the edit row's property picker, then distribute.
    harness
        .get_all_by_value("friction")
        .next()
        .unwrap_or_else(|| panic!("the property picker"))
        .click();
    harness.run();
    harness.get_by_label("density").click();
    harness.run();
    harness.get_by_label("Distribute").click();
    harness.run();
    harness.get_by_label("Random").click();
    harness.run();
    harness.get_by_label("Distribute").click();
    harness.run();
    harness.get_by_label("By kind").click();
    harness.run();
    harness.get_by_label("box").click();
    harness.run();
    harness.get_by_label("Group selection").click();
    harness.run();
    drop(harness);

    let out = out.into_inner();
    let blue = gradiance::domain::appearance::PALETTE[7].color;
    let got: Vec<(String, serde_json::Value)> = out
        .invoke
        .iter()
        .map(|op| (op.id.to_string(), op.params.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "group.set_color".into(),
                serde_json::json!({"group": 1, "r": blue.r, "g": blue.g, "b": blue.b})
            ),
            (
                "group.distribute".into(),
                serde_json::json!({"group": 1, "prop": "density", "lo": 0.5, "hi": 5.0, "by": "x"})
            ),
            (
                "group.scatter".into(),
                serde_json::json!({"group": 1, "prop": "density", "lo": 0.5, "hi": 5.0, "seed": 1})
            ),
            (
                "group.decompose".into(),
                serde_json::json!({"group": 1, "key": "kind"})
            ),
            (
                "group.create".into(),
                serde_json::json!({"targets": ["selection"]})
            ),
        ]
    );
    assert_eq!(
        out.filter,
        Some(GroupsFilter {
            clauses: vec!["kind:box".into()]
        })
    );
}
