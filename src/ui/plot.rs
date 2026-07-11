//! The live plotter: a time-series panel for the selected body's physics.
//!
//! A pure *read* — the plotter introspects `physics::queries` and `Transform`
//! and records a rolling history; it never mutates authored state (invariant
//! #4). This is the visualization half of the read-total governance model
//! (`docs/script-lisp-decision.md` §"Live plotters"): a plotter is just another
//! reader of the same facade scripts read through.
//!
//! `sample_plot` fills the history each frame while playing; `plot_panel`
//! (backquote's neighbour, backslash) draws it. The plot is hand-rendered with
//! the egui painter — no plotting dependency.

use crate::interaction::selection::Selection;
use crate::physics::queries::PhysicsQueries;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use std::collections::VecDeque;

/// Rolling live-signal history for the plot panel. Records the *single* selected
/// body's speed and height; switching bodies (or selecting zero/many) resets it.
#[derive(Resource, Default)]
pub struct PlotHistory {
    tracked: Option<Entity>,
    speed: VecDeque<f32>,
    height: VecDeque<f32>,
}

impl PlotHistory {
    /// Samples kept per signal (~10 s at 60 fps).
    const CAP: usize = 600;

    /// Appends one sample for `entity`, resetting first if the tracked body
    /// changed (so a plot never mixes two bodies' histories).
    fn record(&mut self, entity: Entity, speed: f32, height: f32) {
        if self.tracked != Some(entity) {
            self.tracked = Some(entity);
            self.speed.clear();
            self.height.clear();
        }
        push_capped(&mut self.speed, speed);
        push_capped(&mut self.height, height);
    }

    /// Forgets the tracked body and its history (nothing plottable selected).
    fn clear(&mut self) {
        self.tracked = None;
        self.speed.clear();
        self.height.clear();
    }
}

/// Pushes `value`, dropping the oldest sample past [`PlotHistory::CAP`].
fn push_capped(buf: &mut VecDeque<f32>, value: f32) {
    buf.push_back(value);
    while buf.len() > PlotHistory::CAP {
        buf.pop_front();
    }
}

/// Plot panel visibility.
#[derive(Resource, Default)]
pub struct PlotPanel {
    open: bool,
}

/// Samples the selected body's live speed and height into the history. Gated on
/// `Playing`, so a pause freezes the plot instead of scrolling flat lines.
pub fn sample_plot(
    selection: Res<Selection>,
    transforms: Query<&Transform>,
    physics: PhysicsQueries,
    mut history: ResMut<PlotHistory>,
) {
    // Exactly one selected body is plottable.
    let mut selected = selection.iter();
    let (Some(entity), None) = (selected.next(), selected.next()) else {
        history.clear();
        return;
    };
    let height = transforms.get(entity).map_or(0.0, |t| t.translation.y);
    let speed = physics.velocity_of(entity).map_or(0.0, |(v, _)| v.length());
    history.record(entity, speed, height);
}

/// Renders the live-plot panel (toggle with backslash).
pub fn plot_panel(
    mut contexts: EguiContexts,
    mut panel: ResMut<PlotPanel>,
    history: Res<PlotHistory>,
    keys: Res<ButtonInput<KeyCode>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if keys.just_pressed(KeyCode::Backslash) && !ctx.egui_wants_keyboard_input() {
        panel.open = !panel.open;
    }
    if !panel.open {
        return Ok(());
    }

    let mut open = true;
    egui::Window::new("Live Plot")
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            if history.tracked.is_none() {
                ui.label("Select one body and press Play to plot its speed and height.");
                return;
            }
            draw_series(
                ui,
                "speed (px/s)",
                &history.speed,
                egui::Color32::from_rgb(120, 200, 255),
            );
            ui.add_space(4.0);
            draw_series(
                ui,
                "height (px)",
                &history.height,
                egui::Color32::from_rgb(255, 190, 120),
            );
        });
    panel.open = open;
    Ok(())
}

/// Hand-draws one signal as a line plot auto-scaled to its own min/max.
fn draw_series(ui: &mut egui::Ui, label: &str, data: &VecDeque<f32>, color: egui::Color32) {
    ui.label(label);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
    if data.len() < 2 {
        return;
    }

    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for &v in data {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if !(hi - lo).is_finite() || (hi - lo).abs() < 1e-6 {
        hi = lo + 1.0;
    }

    let n = data.len();
    let map = |i: usize, v: f32| {
        let x = rect.left() + rect.width() * (i as f32 / (n - 1) as f32);
        let y = rect.bottom() - rect.height() * ((v - lo) / (hi - lo));
        egui::pos2(x, y)
    };
    let stroke = egui::Stroke::new(1.5, color);
    let mut prev = map(0, data[0]);
    for (i, &v) in data.iter().enumerate().skip(1) {
        let point = map(i, v);
        painter.line_segment([prev, point], stroke);
        prev = point;
    }

    // Min/max labels in the corners.
    let font = egui::FontId::monospace(9.0);
    painter.text(
        rect.right_top() + egui::vec2(-2.0, 1.0),
        egui::Align2::RIGHT_TOP,
        format!("{hi:.0}"),
        font.clone(),
        egui::Color32::GRAY,
    );
    painter.text(
        rect.right_bottom() + egui::vec2(-2.0, -1.0),
        egui::Align2::RIGHT_BOTTOM,
        format!("{lo:.0}"),
        font,
        egui::Color32::GRAY,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_caps_and_resets_on_body_change() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let mut history = PlotHistory::default();

        for i in 0..(PlotHistory::CAP + 50) {
            history.record(a, i as f32, 0.0);
        }
        assert_eq!(history.speed.len(), PlotHistory::CAP, "capped at CAP");
        assert_eq!(history.tracked, Some(a));

        // Switching bodies drops the old history.
        history.record(b, 1.0, 2.0);
        assert_eq!(history.tracked, Some(b));
        assert_eq!(history.speed.len(), 1, "reset then one fresh sample");

        history.clear();
        assert_eq!(history.tracked, None);
        assert!(history.speed.is_empty());
    }
}
