//! Precision widgets: commit-on-release number fields (the behaviours are
//! `unit_fields::number_field`'s, brief U-8) and the layout vocabulary.
//!
//! Curve-picker widgets (Lightroom-style response curves) and symbolic /
//! equation input are planned extensions of this module: both slot in as
//! new `Commit`-returning widgets without touching callers.

use bevy_egui::egui::{self, Ui};
use gradiance_units::{Dimension, DisplayUnit};

/// Outcome of a committing widget interaction.
pub enum Commit<T> {
    /// Still idle or mid-gesture; nothing to commit.
    None,
    /// A completed edit: `(value_before_gesture, value_now)`.
    Done(T, T),
}

/// A number field on the **edit** seam ([`crate::unit_fields::number_field`]:
/// scrub, Shift = 1/10, Ctrl snaps, Esc cancels, typed maths and units,
/// Backspace / middle-click / Reset = `default`, `-` negates, Ctrl+wheel
/// steps, Copy / Paste) that commits **once per gesture**, returning the
/// pre-gesture value for the undo record:
/// - a drag commits on release, and nothing when Esc cancelled it;
/// - a typed entry commits on Enter or focus loss, and nothing on Esc;
/// - a one-step edit (a key, a wheel notch, a menu row) commits at once.
///
/// For a pure number; [`precise_drag_unit`] is the same for a quantity.
pub fn precise_drag(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
) -> Commit<f32> {
    precise_drag_unit(ui, id, value, default, speed, Dimension::Dimensionless)
}

/// [`precise_drag`] for a physical quantity of dimension `dim`. `value`,
/// `default` and `speed` are base SI; the field shows them in the unit the
/// display preference picks ([`crate::unit_fields`]: degrees or radians,
/// metres, centimetres or millimetres) with the unit after the number.
///
/// Typed entry: a bare number is in the shown unit, a number with a unit
/// converts (`25 cm` in a metre field stores `0.25`), and an entry of the
/// wrong dimension or that does not parse leaves the value as it was.
pub fn precise_drag_unit(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    dim: Dimension,
) -> Commit<f32> {
    let unit = crate::unit_fields::display(ui.ctx(), dim);
    precise_drag_in(ui, id, value, default, speed, unit)
}

/// [`precise_drag_unit`] in a fixed unit rather than the preferred one, for
/// a field whose unit is its own (a hinge motor's speed in rpm).
pub fn precise_drag_in(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    unit: DisplayUnit,
) -> Commit<f32> {
    let start_id = id.with("gesture-start");
    let active_id = id.with("gesture-active");

    // An authored-property field only commits on *release*, so its source
    // (the component) stays at the pre-edit value for the whole gesture and
    // the caller re-seeds `*value` to it every frame. Restore the in-progress
    // value first, or the drag can never accumulate — it resets to the source
    // each frame and the field looks "frozen". (Settings fields update in
    // place, so `active` equals the source and this is a no-op for them.)
    if let Some(active) = ui.data(|d| d.get_temp::<f32>(active_id)) {
        *value = active;
    }

    let before = *value;
    let field = crate::unit_fields::number_field(ui, value, speed, None, unit, Some(default));
    let response = field.response;

    // A one-step edit (Backspace, `-`, Ctrl+wheel, paste, Reset,
    // middle-click) is a whole gesture: commit it now.
    if let Some(old) = field.instant {
        ui.data_mut(|d| {
            d.remove::<f32>(active_id);
            d.remove::<f32>(start_id);
        });
        if (old - *value).abs() > f32::EPSILON {
            return Commit::Done(old, *value);
        }
        return Commit::None;
    }

    // The value before this frame's change: on a drag's first frame egui
    // has already moved it.
    if response.drag_started() || response.gained_focus() {
        ui.data_mut(|d| d.insert_temp(start_id, before));
    }
    // Carry the working value across frames while the gesture is live; drop it
    // the moment it ends so a stale value can't shadow the source.
    if response.dragged() || response.has_focus() {
        ui.data_mut(|d| d.insert_temp(active_id, *value));
    } else {
        ui.data_mut(|d| d.remove::<f32>(active_id));
    }

    let finished = response.drag_stopped() || response.lost_focus();
    if finished {
        let old = ui
            .data_mut(|d| d.get_temp::<f32>(start_id))
            .unwrap_or(*value);
        ui.data_mut(|d| d.remove::<f32>(start_id));
        if (old - *value).abs() > f32::EPSILON {
            return Commit::Done(old, *value);
        }
    }
    Commit::None
}

// ---------------------------------------------------------------------------
// Layout vocabulary
// ---------------------------------------------------------------------------
//
// These exist because the crate had grown four different ways to write a
// section header, two spellings of the close button (neither of which
// rendered — see `crate::fonts`), and no shared notion of a labelled row or an
// empty state at all. Each panel had reinvented them slightly differently, so
// nothing lined up between panes.
//
// They are deliberately thin. The goal is one obvious way to say a common
// thing, not an abstraction layer over egui.

/// A section heading.
///
/// The crate previously used `RichText::strong`, `RichText::weak`,
/// `ui.heading`, and bare `CollapsingHeader` for the same job, so headings
/// changed weight from pane to pane. This is the one spelling.
///
/// Returns the `Response` so a header can still carry hover text, which
/// several of them do.
pub fn section_header(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).strong())
}

/// Explanatory text under a header or beside a control.
///
/// Small and dimmed — for the sentence that says what a section is *for*,
/// which several panes were writing as a full-weight label.
pub fn hint(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).weak().small())
}

/// The placeholder shown when a panel has nothing to display.
///
/// Five panes phrased these differently and two used a full-weight `label`,
/// so an empty panel read as either an error or a heading depending on which
/// one you were looking at.
pub fn empty_state(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).weak())
}

/// A small close / remove button.
///
/// One glyph and one size for an action that had two of each. Returns whether
/// it was clicked.
pub fn close_button(ui: &mut Ui, hover: &str) -> bool {
    ui.small_button(crate::fonts::glyph::CLOSE)
        .on_hover_text(hover)
        .clicked()
}

/// A labelled numeric row inside an [`egui::Grid`]: label, drag, `end_row`.
///
/// Generalised from the optimizer's private helper, which was already the
/// right shape and the only place in the crate where labels lined up into a
/// column. Everywhere else builds rows as ad-hoc `ui.horizontal`, so labels
/// never align — using this is what makes a panel look like the rest.
///
/// For **authored** values prefer [`precise_drag_unit`], which commits once
/// per gesture for the undo record; this is for config-seam edits, which are
/// not undoable.
pub fn labelled_drag(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f32,
    hover: &str,
) -> bool {
    ui.label(label).on_hover_text(hover);
    let plain = DisplayUnit::canonical(Dimension::Dimensionless);
    let changed = crate::unit_fields::stored_drag(ui, value, Some(range), speed, plain).changed();
    ui.end_row();
    changed
}

/// [`labelled_drag`] for a physical quantity: `value`, `range` and `speed`
/// are in `stored` (usually [`DisplayUnit::canonical`]; a settings field
/// kept in degrees passes [`DisplayUnit::DEGREES`]), and the field shows and
/// parses in the unit the display preference picks for `stored`'s
/// dimension ([`crate::unit_fields`]). The config seam: no gesture
/// bookkeeping, "changed" every frame it moves.
pub fn labelled_quantity(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    speed: f32,
    hover: &str,
    stored: DisplayUnit,
) -> bool {
    ui.label(label).on_hover_text(hover);
    let changed = crate::unit_fields::stored_drag(ui, value, Some(range), speed, stored).changed();
    ui.end_row();
    changed
}

/// The same, for an integer.
pub fn labelled_drag_u32(
    ui: &mut Ui,
    label: &str,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    hover: &str,
) -> bool {
    ui.label(label).on_hover_text(hover);
    let changed = ui
        .add(egui::DragValue::new(value).speed(0.2).range(range))
        .changed();
    ui.end_row();
    changed
}

/// The behavior-node kind editor widgets.
///
/// Lives here rather than beside any one host because it has three: the
/// inspector, the node context menu, and the signal list. Returns the edited
/// kind when a field changed (the caller emits one undoable
/// `PropertyEditIntent` — this widget never mutates). `salt` keeps widget ids
/// unique between host surfaces.
pub fn node_kind_editor(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &gradiance_domain::node::NodeKind,
) -> Option<gradiance_domain::node::NodeKind> {
    use gradiance_domain::node::NodeKind;
    let mut next = kind.clone();
    match &mut next {
        NodeKind::Tracer(tracer) => {
            ui.horizontal(|ui| {
                ui.label("fade (s)");
                ui.add(egui::DragValue::new(&mut tracer.fade_secs.0).speed(0.05));
                ui.label("size");
                ui.add(
                    egui::DragValue::new(&mut tracer.size.0)
                        .speed(0.1)
                        .range(0.5..=20.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("pattern");
                egui::ComboBox::from_id_salt(ui.id().with((salt, "pattern")))
                    .selected_text(format!("{:?}", tracer.pattern))
                    .show_ui(ui, |ui| {
                        for p in gradiance_domain::tracer::TracePattern::ALL {
                            ui.selectable_value(&mut tracer.pattern, p, format!("{p:?}"));
                        }
                    });
            });
        }
    }
    (next != *kind).then_some(next)
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    /// The helpers must not panic on an empty context — they are called from
    /// every pane, including headless where there is no real UI.
    #[test]
    fn the_layout_helpers_render_headlessly() {
        // egui's own test harness: builds a real `Ui` without a window, which
        // is the path every headless panel test takes.
        egui::__run_test_ui(|ui| {
            let _ = section_header(ui, "Section");
            let _ = hint(ui, "what this is for");
            let _ = empty_state(ui, "Nothing selected");
            let _ = close_button(ui, "remove");
            egui::Grid::new("t").num_columns(2).show(ui, |ui| {
                let mut v = 1.0_f32;
                let _ = labelled_drag(ui, "value", &mut v, 0.0..=2.0, 0.01, "hover");
                let mut n = 3_u32;
                let _ = labelled_drag_u32(ui, "count", &mut n, 1..=9, "hover");
            });
        });
    }
}
