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
        "Line (L)",
        "Arc (A)",
        "Trim (T)",
        "Hinge (H)",
        "Prismatic (R)",
        "Strut (T)",
        "Weld (W)",
        "Ground (G)",
        "Cut (K)",
        "Tracer (Y)",
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

/// The unified palette lists the sketch tools alongside the rest — there is no
/// separate sketch palette to switch into any more.
#[test]
fn tool_palette_includes_the_sketch_tools() {
    let clicked: Cell<Option<ToolState>> = Cell::new(None);
    let mut harness = Harness::new_ui(|ui| {
        if let Some(tool) = tools_palette_ui(ui, ToolState::Select, None) {
            clicked.set(Some(tool));
        }
    });
    harness.run();

    for label in ["Box (B)", "Circle (C)", "Line (L)", "Arc (A)", "Trim (T)"] {
        harness.get_by_label(label);
    }

    harness.get_by_label("Arc (A)").click();
    harness.run();
    assert_eq!(clicked.get(), Some(ToolState::Arc));
}

/// The sketch strip carries what has no home in the tool palette: the
/// reference toggle and the degrees-of-freedom readout.
#[test]
fn sketch_strip_reports_dof_and_toggles_reference_geometry() {
    use gradiance::ui::toolbar::{SketchAction, sketch_palette_ui};

    let clicked: Cell<Option<SketchAction>> = Cell::new(None);
    let mut harness = Harness::new_ui(|ui| {
        if let Some(a) = sketch_palette_ui(ui, Some(3), false) {
            clicked.set(Some(a));
        }
    });
    harness.run();

    harness.get_by_label("3 DOF");
    harness.get_by_label("Ref").click();
    harness.run();
    assert_eq!(clicked.get(), Some(SketchAction::SetConstruction(true)));
}

/// A fully constrained sketch says so rather than reporting "0 DOF".
#[test]
fn sketch_strip_calls_out_a_fully_constrained_sketch() {
    use gradiance::ui::toolbar::sketch_palette_ui;

    let mut harness = Harness::new_ui(|ui| {
        sketch_palette_ui(ui, Some(0), false);
    });
    harness.run();
    harness.get_by_label("fully constrained");
}

/// With nothing selected the editor panel offers no constraints, and says why.
#[test]
fn sketch_editor_offers_nothing_for_an_empty_selection() {
    use gradiance::ui::sketch_panel::{SketchPanel, SketchView, sketch_editor_ui};

    let mut panel = SketchPanel::default();
    let mut harness = Harness::new_ui(move |ui| {
        let view = SketchView {
            applicable: &[],
            constraints: &[],
            failed: &[],
            dof: None,
            status: None,
            points: 0,
            entities: 0,
            can_commit: false,
        };
        sketch_editor_ui(ui, &view, &mut panel);
    });
    harness.run();

    harness.get_by_label("nothing selected");
    harness.get_by_label("select geometry to see what can be constrained");
}

/// The panel offers exactly the constraints that apply to the selection, and
/// reports which one was asked for.
#[test]
fn sketch_editor_offers_applicable_constraints_and_reports_clicks() {
    use gradiance::sketch::edit::ConstraintKind;
    use gradiance::ui::sketch_panel::{
        SketchPanel, SketchPanelAction, SketchView, sketch_editor_ui,
    };

    let clicked: Cell<Option<SketchPanelAction>> = Cell::new(None);
    let mut panel = SketchPanel::default();
    let mut harness = Harness::new_ui(|ui| {
        let view = SketchView {
            applicable: &[ConstraintKind::Parallel, ConstraintKind::Perpendicular],
            constraints: &[],
            failed: &[],
            dof: Some(4),
            status: None,
            points: 0,
            entities: 2,
            can_commit: false,
        };
        if let Some(a) = sketch_editor_ui(ui, &view, &mut panel) {
            clicked.set(Some(a));
        }
    });
    harness.run();

    harness.get_by_label("Parallel");
    harness.get_by_label("Perpendicular");
    harness.get_by_label("0 point(s), 2 edge(s)");

    harness.get_by_label("Parallel").click();
    harness.run();
    assert_eq!(
        clicked.get(),
        Some(SketchPanelAction::Constrain(ConstraintKind::Parallel, None)),
        "a relational constraint carries no measurement"
    );
}

/// A dimension carries the panel's value; a relational constraint does not.
#[test]
fn sketch_editor_sends_a_value_with_dimensions_only() {
    use gradiance::sketch::edit::ConstraintKind;
    use gradiance::ui::sketch_panel::{
        SketchPanel, SketchPanelAction, SketchView, sketch_editor_ui,
    };

    let clicked: Cell<Option<SketchPanelAction>> = Cell::new(None);
    let mut panel = SketchPanel {
        value: 2.5,
        ..SketchPanel::default()
    };
    let mut harness = Harness::new_ui(|ui| {
        let view = SketchView {
            applicable: &[ConstraintKind::Distance],
            constraints: &[],
            failed: &[],
            dof: Some(2),
            status: None,
            points: 2,
            entities: 0,
            can_commit: false,
        };
        if let Some(a) = sketch_editor_ui(ui, &view, &mut panel) {
            clicked.set(Some(a));
        }
    });
    harness.run();

    // Dimensions are marked as wanting input rather than applying instantly.
    harness.get_by_label("Distance …").click();
    harness.run();
    assert_eq!(
        clicked.get(),
        Some(SketchPanelAction::Constrain(
            ConstraintKind::Distance,
            Some(2.5)
        ))
    );
}

/// Constraints the solver rejected are listed and removable, which is the only
/// way to recover from over-constraining a sketch.
#[test]
fn sketch_editor_lists_constraints_and_removes_them() {
    use gradiance::sketch::doc::{SketchConstraint, SketchId};
    use gradiance::ui::sketch_panel::{
        SketchPanel, SketchPanelAction, SketchView, sketch_editor_ui,
    };

    let clicked: Cell<Option<SketchPanelAction>> = Cell::new(None);
    let mut panel = SketchPanel::default();
    let constraints = [
        SketchConstraint::Horizontal(SketchId(1)),
        SketchConstraint::Vertical(SketchId(2)),
    ];
    let mut harness = Harness::new_ui(|ui| {
        let view = SketchView {
            applicable: &[],
            constraints: &constraints,
            // The second one is unsatisfiable, and has to say so.
            failed: &[1],
            dof: Some(1),
            status: None,
            points: 0,
            entities: 0,
            can_commit: true,
        };
        if let Some(a) = sketch_editor_ui(ui, &view, &mut panel) {
            clicked.set(Some(a));
        }
    });
    harness.run();

    harness.get_by_label("Constraints (2)");
    harness.get_by_label("horizontal");
    harness.get_by_label("vertical");

    harness.get_all_by_label("✕").next().unwrap().click();
    harness.run();
    assert_eq!(clicked.get(), Some(SketchPanelAction::RemoveConstraint(0)));
}

/// Commit is offered only when the profile actually lowers to a body.
#[test]
fn sketch_editor_gates_commit_on_a_closed_profile() {
    use gradiance::ui::sketch_panel::{
        SketchPanel, SketchPanelAction, SketchView, sketch_editor_ui,
    };

    let clicked: Cell<Option<SketchPanelAction>> = Cell::new(None);
    let mut panel = SketchPanel::default();
    let mut harness = Harness::new_ui(|ui| {
        let view = SketchView {
            applicable: &[],
            constraints: &[],
            failed: &[],
            dof: Some(0),
            status: None,
            points: 4,
            entities: 4,
            can_commit: true,
        };
        if let Some(a) = sketch_editor_ui(ui, &view, &mut panel) {
            clicked.set(Some(a));
        }
    });
    harness.run();

    harness.get_by_label("✔ Commit").click();
    harness.run();
    assert_eq!(clicked.get(), Some(SketchPanelAction::Commit));
}
