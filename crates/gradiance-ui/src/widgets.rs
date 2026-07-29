//! Precision widgets: commit-on-release drags with scientific notation
//! and middle-click default reset.
//!
//! Curve-picker widgets (Lightroom-style response curves) and symbolic /
//! equation input are planned extensions of this module: both slot in as
//! new `Commit`-returning widgets without touching callers.

use bevy_egui::egui::{self, Ui};

/// Outcome of a committing widget interaction.
pub enum Commit<T> {
    /// Still idle or mid-gesture; nothing to commit.
    None,
    /// A completed edit: `(value_before_gesture, value_now)`.
    Done(T, T),
}

/// A drag value that:
/// - parses scientific notation (`1.5e3`) and plain floats,
/// - resets to `default` on **middle-click**,
/// - commits once per gesture (drag release / focus loss), returning the
///   pre-gesture value for the undo record.
pub fn precise_drag(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
) -> Commit<f32> {
    precise_drag_unit(ui, id, value, default, speed, "")
}

/// [`precise_drag`] with a trailing SI unit shown inside the field (e.g.
/// `0.20 m`). Pass `""` for a dimensionless value. The unit is display-only —
/// the stored value is always the base-SI magnitude.
pub fn precise_drag_unit(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    unit: &str,
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

    let suffix = if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    };
    // Typed input tolerates the shown unit (`0.2 m`, `0.2m`, or `0.2`) — the
    // canonical-unit contract of `units::Dimension::parse`. The value stored is
    // always the base-SI magnitude.
    let unit_owned = unit.to_owned();
    let response = ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .suffix(suffix)
            .custom_parser(move |text| parse_with_unit(text, &unit_owned)),
    );

    // Middle-click: reset to default, committed immediately.
    if response.clicked_by(egui::PointerButton::Middle) {
        ui.data_mut(|d| d.remove::<f32>(active_id));
        let old = *value;
        *value = default;
        if (old - default).abs() > f32::EPSILON {
            return Commit::Done(old, default);
        }
        return Commit::None;
    }

    if response.drag_started() || response.gained_focus() {
        ui.data_mut(|d| d.insert_temp(start_id, *value));
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

/// Parses a number that may carry the field's own SI unit suffix (`"0.2 m"`,
/// `"0.2m"`, or `"0.2"`). A wrong or partial unit makes the parse fail, so the
/// widget keeps its previous value. Pure — unit-tested below.
fn parse_with_unit(text: &str, unit: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_suffix(unit).unwrap_or(t).trim_end();
    t.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_with_unit;

    #[test]
    fn typed_input_tolerates_the_field_unit() {
        // Bare number, with-space unit, and no-space unit all parse.
        assert_eq!(parse_with_unit("0.2", "m"), Some(0.2));
        assert_eq!(parse_with_unit("0.2 m", "m"), Some(0.2));
        assert_eq!(parse_with_unit("0.2m", "m"), Some(0.2));
        assert_eq!(parse_with_unit("1.5 m/s", "m/s"), Some(1.5));
        assert_eq!(parse_with_unit("2.5 kg/m²", "kg/m²"), Some(2.5));
        // A dimensionless field (empty unit) still parses a bare number.
        assert_eq!(parse_with_unit("0.5", ""), Some(0.5));
        // Garbage or a mismatched unit fails (widget keeps its value).
        assert_eq!(parse_with_unit("abc", "m"), None);
        assert_eq!(parse_with_unit("1.5 m/s", "m"), None);
    }
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
    let changed = ui
        .add(egui::DragValue::new(value).speed(speed).range(range))
        .changed();
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
