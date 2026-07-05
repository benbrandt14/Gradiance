//! Precision widgets: commit-on-release drags with scientific notation
//! and middle-click default reset; range selectors for `[min, max]` pairs.
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
    let start_id = id.with("gesture-start");
    let response = ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .custom_parser(|text| text.trim().parse::<f64>().ok()),
    );

    // Middle-click: reset to default, committed immediately.
    if response.clicked_by(egui::PointerButton::Middle) {
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

/// A `[min, max]` range editor (two committing drags that keep order).
pub fn range_selector(
    ui: &mut Ui,
    id: egui::Id,
    range: &mut [f32; 2],
    default: [f32; 2],
    speed: f32,
) -> Commit<[f32; 2]> {
    let mut committed: Option<[f32; 2]> = None;
    let before = *range;
    ui.horizontal(|ui| {
        if let Commit::Done(old, _) = precise_drag(ui, id.with(0), &mut range[0], default[0], speed)
        {
            committed = Some([old, before[1]]);
        }
        ui.label("..");
        if let Commit::Done(old, _) = precise_drag(ui, id.with(1), &mut range[1], default[1], speed)
        {
            committed = Some([before[0], old]);
        }
    });
    if range[0] > range[1] {
        range.swap(0, 1);
    }
    match committed {
        Some(old) => Commit::Done(old, *range),
        None => Commit::None,
    }
}
