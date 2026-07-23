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
    let response = ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .suffix(suffix)
            .custom_parser(|text| text.trim().parse::<f64>().ok()),
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
