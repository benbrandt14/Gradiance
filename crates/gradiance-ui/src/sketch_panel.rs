//! The sketch editor panel: constraints, operations, and the commit.
//!
//! Sketch mode's tools can only *draw*. Everything that makes a sketch
//! parametric rather than merely tidy — saying "these two are parallel", giving
//! an edge a dimension, rounding a corner, deciding a line is reference only —
//! is an operation on the current **selection**, not a gesture. So it lives on
//! a panel, and the panel is the reason the constraint vocabulary is reachable
//! at all.
//!
//! # What the panel offers is derived, not enumerated
//!
//! The constraint buttons come from `edit::applicable`, which reads the
//! selection and reports what would make sense. Two lines offer Parallel and
//! Perpendicular; a point and a circle offer Point-on-circle. Nothing is shown
//! that would immediately fail, which is what stops the panel from being a
//! wall of greyed-out verbs.
//!
//! # Seams
//!
//! [`sketch_editor_ui`] is pure — a borrowed [`SketchView`] in, an optional
//! [`SketchPanelAction`] out — so `tests/it/ui_panels.rs` can drive it
//! headlessly, the same shape the tool palette uses. The hosting system is the
//! only part that touches ECS, and it never writes an authored component: it
//! edits the sketch document (preview state, invariant 2) and *requests* a
//! commit, which the session turns into exactly one intent.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use gradiance_core::states::EditorMode;
use gradiance_interaction::tools::sketch_session::{
    SessionStatus, SketchOp, SketchSession, describe_constraint,
};
use gradiance_sketch::doc::SketchConstraint;
use gradiance_sketch::edit::ConstraintKind;

/// Panel-local scratch: the numbers the author is about to use.
///
/// Editor state, never persisted — these are the values sitting in the spin
/// boxes, not anything the document remembers.
#[derive(Resource, Debug)]
pub struct SketchPanel {
    /// Whether the panel is showing.
    pub open: bool,
    /// The measurement applied to dimension constraints.
    pub value: f32,
    /// Fillet radius.
    pub fillet_radius: f32,
    /// Chamfer setback.
    pub chamfer_setback: f32,
    /// Offset distance; negative offsets to the other side.
    pub offset_distance: f32,
}

impl Default for SketchPanel {
    fn default() -> Self {
        Self {
            open: true,
            // Sketch units are metres, so these are centimetre-scale defaults —
            // small enough to apply to a hand-drawn sketch without swallowing it.
            value: 1.0,
            fillet_radius: 0.1,
            chamfer_setback: 0.1,
            offset_distance: 0.1,
        }
    }
}

/// Everything the panel renders, borrowed from the session.
///
/// A view rather than the session itself so the widget stays pure and
/// testable: a test can describe a selection state directly instead of having
/// to drive gestures to produce one.
pub struct SketchView<'a> {
    /// Constraints that would apply to the current selection.
    pub applicable: &'a [ConstraintKind],
    /// Every constraint on the document, in index order.
    pub constraints: &'a [SketchConstraint],
    /// Indices into `constraints` the solver could not satisfy.
    pub failed: &'a [usize],
    /// Remaining degrees of freedom.
    pub dof: Option<i32>,
    /// The last action's outcome.
    pub status: Option<&'a SessionStatus>,
    /// How many points are selected.
    pub points: usize,
    /// How many entities are selected.
    pub entities: usize,
    /// Whether the profile currently lowers to a body.
    pub can_commit: bool,
}

/// What the panel is asking for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SketchPanelAction {
    /// Attach a constraint to the selection.
    Constrain(ConstraintKind, Option<f32>),
    /// Drop the constraint at this index.
    RemoveConstraint(usize),
    /// Run a selection-driven operation.
    Op(SketchOp),
    /// Deselect everything.
    ClearSelection,
    /// Turn the sketch into a body.
    Commit,
    /// Throw the sketch away.
    Discard,
}

/// Host the sketch editor panel while sketch mode is active.
///
/// # Errors
///
/// Propagates the egui context lookup.
pub fn sketch_panel(
    mut contexts: EguiContexts,
    mode: Res<State<EditorMode>>,
    mut panel: ResMut<SketchPanel>,
    mut session: ResMut<SketchSession>,
) -> Result {
    if *mode.get() != EditorMode::Sketch || !panel.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;

    let applicable = session.applicable();
    let action = {
        let view = SketchView {
            applicable: &applicable,
            constraints: &session.doc().constraints,
            failed: session.failed(),
            dof: session.dof(),
            status: session.status(),
            points: session.selection().points.len(),
            entities: session.selection().entities.len(),
            can_commit: session.can_commit(),
        };
        let mut action = None;
        egui::Window::new("Sketch")
            .anchor(egui::Align2::RIGHT_TOP, [-8.0, 120.0])
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                action = sketch_editor_ui(ui, &view, &mut panel);
            });
        action
    };

    match action {
        Some(SketchPanelAction::Constrain(kind, value)) => session.apply_constraint(kind, value),
        Some(SketchPanelAction::RemoveConstraint(i)) => session.remove_constraint(i),
        Some(SketchPanelAction::Op(op)) => session.run_op(op),
        Some(SketchPanelAction::ClearSelection) => session.clear_selection(),
        Some(SketchPanelAction::Commit) => session.request_commit(),
        Some(SketchPanelAction::Discard) => session.abandon(),
        None => {}
    }
    Ok(())
}

/// The panel body: pure `Ui` in, choice out.
pub fn sketch_editor_ui(
    ui: &mut egui::Ui,
    view: &SketchView,
    panel: &mut SketchPanel,
) -> Option<SketchPanelAction> {
    let mut action = None;

    selection_header(ui, view, &mut action);
    ui.separator();
    constraint_section(ui, view, panel, &mut action);
    ui.separator();
    operation_section(ui, view, panel, &mut action);
    ui.separator();
    constraint_list(ui, view, &mut action);
    ui.separator();
    footer(ui, view, &mut action);

    action
}

/// What is selected, and the escape hatch from it.
fn selection_header(ui: &mut egui::Ui, view: &SketchView, action: &mut Option<SketchPanelAction>) {
    ui.horizontal(|ui| {
        if view.points == 0 && view.entities == 0 {
            ui.weak("nothing selected");
        } else {
            ui.label(format!(
                "{} point(s), {} edge(s)",
                view.points, view.entities
            ));
            if ui.small_button("clear").clicked() {
                *action = Some(SketchPanelAction::ClearSelection);
            }
        }
    });
    ui.weak("click to add or remove; empty space clears");
}

/// The constraints that apply to what is selected, and nothing else.
fn constraint_section(
    ui: &mut egui::Ui,
    view: &SketchView,
    panel: &mut SketchPanel,
    action: &mut Option<SketchPanelAction>,
) {
    ui.strong("Constrain");
    if view.applicable.is_empty() {
        ui.weak("select geometry to see what can be constrained");
        return;
    }

    // Dimensions need a measurement, so the value box appears only when one of
    // the offered constraints would actually consume it.
    let wants_value = view.applicable.iter().any(|k| k.is_dimension());
    if wants_value {
        ui.horizontal(|ui| {
            ui.label("value");
            ui.add(
                egui::DragValue::new(&mut panel.value)
                    .speed(0.01)
                    .range(0.0..=f32::MAX),
            )
            .on_hover_text("metres for distances, degrees for angles");
        });
    }

    ui.horizontal_wrapped(|ui| {
        for &kind in view.applicable {
            let label = if kind.is_dimension() {
                format!("{} …", kind.label())
            } else {
                kind.label().to_owned()
            };
            if ui
                .button(label)
                .on_hover_text(constraint_hint(kind))
                .clicked()
            {
                let value = kind.is_dimension().then_some(panel.value);
                *action = Some(SketchPanelAction::Constrain(kind, value));
            }
        }
    });
}

/// Why an author would reach for each constraint.
fn constraint_hint(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::Coincident => "make the two points the same point",
        ConstraintKind::PointOnLine => "keep the point sliding along the line",
        ConstraintKind::PointOnCircle => "keep the point on the rim",
        ConstraintKind::Midpoint => "hold the point at the centre of the edge",
        ConstraintKind::Horizontal => "lock the edge to the horizontal axis",
        ConstraintKind::Vertical => "lock the edge to the vertical axis",
        ConstraintKind::Parallel => "keep the two edges in the same direction",
        ConstraintKind::Perpendicular => "hold the two edges at a right angle",
        ConstraintKind::Tangent => "meet smoothly, with no corner",
        ConstraintKind::EqualLength => "tie the two edges to one length",
        ConstraintKind::EqualRadius => "tie the two radii together",
        ConstraintKind::Distance => "fix the gap between the two points",
        ConstraintKind::PointLineDistance => "hold the point a set distance off the line",
        ConstraintKind::Diameter => "fix the diameter",
        ConstraintKind::Angle => "fix the angle between the two edges, in degrees",
        ConstraintKind::Symmetric => "mirror the two points about the edge",
    }
}

/// Geometry edits that act on the selection.
fn operation_section(
    ui: &mut egui::Ui,
    view: &SketchView,
    panel: &mut SketchPanel,
    action: &mut Option<SketchPanelAction>,
) {
    ui.strong("Modify");
    let has_points = view.points > 0;
    let has_entities = view.entities > 0;

    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut panel.fillet_radius)
                .speed(0.01)
                .range(0.0..=f32::MAX)
                .prefix("r "),
        );
        if ui
            .add_enabled(has_points, egui::Button::new("Fillet"))
            .on_hover_text("round each selected corner with a tangent arc")
            .clicked()
        {
            *action = Some(SketchPanelAction::Op(SketchOp::Fillet {
                radius: panel.fillet_radius,
            }));
        }
    });

    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut panel.chamfer_setback)
                .speed(0.01)
                .range(0.0..=f32::MAX)
                .prefix("d "),
        );
        if ui
            .add_enabled(has_points, egui::Button::new("Chamfer"))
            .on_hover_text("cut each selected corner back to a straight edge")
            .clicked()
        {
            *action = Some(SketchPanelAction::Op(SketchOp::Chamfer {
                setback: panel.chamfer_setback,
            }));
        }
    });

    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut panel.offset_distance)
                .speed(0.01)
                .prefix("d "),
        )
        .on_hover_text("negative offsets to the other side");
        if ui
            .add_enabled(has_entities, egui::Button::new("Offset"))
            .on_hover_text("copy the selected chain over, mitring the joints")
            .clicked()
        {
            *action = Some(SketchPanelAction::Op(SketchOp::Offset {
                distance: panel.offset_distance,
            }));
        }
    });

    ui.horizontal(|ui| {
        if ui
            .add_enabled(has_entities, egui::Button::new("Reference"))
            .on_hover_text(
                "toggle between profile and reference geometry — reference edges \
                 are solved and snappable but never become part of the body",
            )
            .clicked()
        {
            *action = Some(SketchPanelAction::Op(SketchOp::ToggleConstruction));
        }
        if ui
            .add_enabled(has_points || has_entities, egui::Button::new("🗑 Delete"))
            .on_hover_text("delete the selection, and anything that depended on it")
            .clicked()
        {
            *action = Some(SketchPanelAction::Op(SketchOp::Delete));
        }
    });
}

/// Every constraint on the document, with the failing ones called out.
///
/// A solver that cannot satisfy a constraint has to say *which* one, or the
/// sketch just mysteriously refuses to move. This list is where that lands, and
/// it is also the only way to undo a constraint applied by mistake.
fn constraint_list(ui: &mut egui::Ui, view: &SketchView, action: &mut Option<SketchPanelAction>) {
    ui.strong(format!("Constraints ({})", view.constraints.len()));
    if view.constraints.is_empty() {
        ui.weak("none yet — drawn segments pick up axis constraints on their own");
        return;
    }
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            for (i, c) in view.constraints.iter().enumerate() {
                let failed = view.failed.contains(&i);
                ui.horizontal(|ui| {
                    if ui
                        .small_button("✕")
                        .on_hover_text("remove this constraint")
                        .clicked()
                    {
                        *action = Some(SketchPanelAction::RemoveConstraint(i));
                    }
                    let text = describe_constraint(c);
                    if failed {
                        ui.colored_label(egui::Color32::LIGHT_RED, text)
                            .on_hover_text("the solver cannot satisfy this one");
                    } else {
                        ui.label(text);
                    }
                });
            }
        });
}

/// Status, degrees of freedom, and the two ways out.
fn footer(ui: &mut egui::Ui, view: &SketchView, action: &mut Option<SketchPanelAction>) {
    crate::toolbar::dof_readout(ui, view.dof);

    if let Some(status) = view.status {
        let color = if status.error {
            egui::Color32::LIGHT_RED
        } else {
            egui::Color32::LIGHT_GREEN
        };
        ui.colored_label(color, &status.text);
    }

    ui.horizontal(|ui| {
        if ui
            .add_enabled(view.can_commit, egui::Button::new("✔ Commit"))
            .on_hover_text(if view.can_commit {
                "turn the closed profile into a body"
            } else {
                "the profile has to be a closed loop before it can become a body"
            })
            .clicked()
        {
            *action = Some(SketchPanelAction::Commit);
        }
        if ui
            .button("Discard")
            .on_hover_text("throw the sketch away")
            .clicked()
        {
            *action = Some(SketchPanelAction::Discard);
        }
    });
}
