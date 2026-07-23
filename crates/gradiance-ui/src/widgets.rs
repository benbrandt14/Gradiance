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
