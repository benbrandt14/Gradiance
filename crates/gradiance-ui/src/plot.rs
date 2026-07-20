//! The live plotter: a time-series panel drawn **entirely from the
//! [`SignalBus`]** (`docs/signal-dataflow.md`).
//!
//! There is one history in the system — the bus. To plot a quantity you wire
//! it to the **plot sink**: a `SignalSink::Plot` binding publishes its source
//! on the bus under its name, and this panel draws every bus signal with a
//! recorded history. The inspector's sensor ports have a one-click "plot"
//! toggle that adds/removes such a binding, so plotting a body's speed is a
//! click. Recording pauses with the simulation (the bus records only while
//! playing). Hand-rendered with the egui painter — no plotting dependency.
//!
//! [`SignalBus`]: gradiance_signal::SignalBus

use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::{HashSet, VecDeque};

/// Plot panel visibility.
#[derive(Resource, Default)]
pub struct PlotPanel {
    open: bool,
}

/// What the plot's horizontal axis represents. Time (sample order, the live
/// default) today; the enum leaves room for "versus another signal" (XY) later.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum XAxis {
    /// Elapsed time — the recorded sample order.
    #[default]
    Time,
}

impl XAxis {
    fn label(self) -> &'static str {
        match self {
            Self::Time => "t",
        }
    }
}

/// Plotter configuration: which series are hidden (default: show all) and what
/// the X axis is. A signal is drawn unless the user unchecks it, so new signals
/// appear automatically. Editor view-state — never persisted.
#[derive(Resource, Default)]
pub struct PlotConfig {
    hidden: HashSet<String>,
    x_axis: XAxis,
}

/// The plottable series to draw, in bus order, minus the ones the user hid —
/// pure, so the selection logic is unit-testable without a window.
fn visible_series<'a>(
    names: impl IntoIterator<Item = &'a str>,
    hidden: &HashSet<String>,
) -> Vec<&'a str> {
    names
        .into_iter()
        .filter(|n| is_plottable(n) && !hidden.contains(*n))
        .collect()
}

impl PlotPanel {
    /// Whether the panel is currently shown (read by the transport button so it
    /// can reflect the open state).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the panel's visibility. Both the `\` shortcut and the transport
    /// button toggle through here.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Colours cycled across the plotted signals.
const SIGNAL_COLORS: [egui::Color32; 4] = [
    egui::Color32::from_rgb(120, 200, 255),
    egui::Color32::from_rgb(255, 190, 120),
    egui::Color32::from_rgb(150, 230, 150),
    egui::Color32::from_rgb(230, 150, 230),
];

/// Every named bus signal the plotter can draw (name + recorded history), in
/// bus order — params, computed signals, and plot-sink bindings. The canonical
/// sensor-ref names (`speed@<uuid>`) that `publish_sensor_refs` puts on the bus
/// for modulation operands are internal plumbing, not user-chosen series, so
/// `is_plottable` drops them. The dock host computes this from the bus and hands
/// it to [`plot_section`].
pub fn plottable_series(bus: &gradiance_signal::SignalBus) -> Vec<(&str, &VecDeque<f32>)> {
    bus.entries()
        .filter(|(name, e)| e.history().len() >= 2 && is_plottable(name))
        .map(|(name, e)| (name, e.history()))
        .collect()
}

/// Renders the plotter's content into `ui` (the **Live Plot** dock pane): a
/// **signals** picker toggling each series, then every visible plottable signal
/// drawn from its recorded bus history, and the X-axis label. `plottable` is the
/// current [`plottable_series`]; the panel's open/toggle handling lives in the
/// bottom-dock host.
pub fn plot_section(
    ui: &mut egui::Ui,
    plottable: &[(&str, &VecDeque<f32>)],
    config: &mut PlotConfig,
) {
    if plottable.is_empty() {
        ui.label(
            "Wire a sensor to the plot sink (a body's ▸plot toggle, or a \
             plot binding) and press Play.",
        );
        return;
    }
    signal_picker(ui, plottable, config);

    let visible = visible_series(plottable.iter().map(|(n, _)| *n), &config.hidden);
    if visible.is_empty() {
        ui.weak("No series selected — pick one above.");
        return;
    }
    for (i, name) in visible.iter().enumerate() {
        if i > 0 {
            ui.add_space(4.0);
        }
        if let Some((_, samples)) = plottable.iter().find(|(n, _)| n == name) {
            draw_series(ui, name, samples, SIGNAL_COLORS[i % SIGNAL_COLORS.len()]);
        }
    }
    ui.add_space(2.0);
    ui.weak(format!("x-axis: {} →", config.x_axis.label()));
}

/// A collapsible list of the plottable signals, each a checkbox that hides or
/// shows its series (unchecking adds the name to [`PlotConfig::hidden`]).
fn signal_picker(ui: &mut egui::Ui, plottable: &[(&str, &VecDeque<f32>)], config: &mut PlotConfig) {
    ui.collapsing("signals", |ui| {
        for (name, _) in plottable {
            let mut shown = !config.hidden.contains(*name);
            if ui.checkbox(&mut shown, *name).changed() {
                if shown {
                    config.hidden.remove(*name);
                } else {
                    config.hidden.insert((*name).to_owned());
                }
            }
        }
    });
}

/// Whether a bus signal is a user-facing series the plot should draw. Params,
/// computed signals, and plot-sink bindings qualify; the canonical sensor-ref
/// names (`speed@<uuid>`) `publish_sensor_refs` surfaces for modulation
/// operands are internal plumbing and stay out of the plot.
fn is_plottable(name: &str) -> bool {
    gradiance_signal::SignalSource::from_bus_name(name).is_none()
}

/// Hand-draws one signal as a line plot auto-scaled to its own min/max.
/// Host-agnostic (pure `Ui` + data in) so `tests/it/ui_panels.rs` can
/// exercise it under `egui_kittest`.
pub fn draw_series(ui: &mut egui::Ui, label: &str, data: &VecDeque<f32>, color: egui::Color32) {
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
    use super::{is_plottable, visible_series};
    use gradiance_core::ids::StableId;
    use gradiance_signal::SignalSource;
    use std::collections::HashSet;

    #[test]
    fn sensor_ref_names_stay_out_of_the_plot() {
        // User-facing named signals plot; internal sensor-ref plumbing doesn't.
        assert!(is_plottable("speed"));
        assert!(is_plottable("my-param"));
        assert!(is_plottable("wire-1"));
        let sensor_ref = SignalSource::Speed(StableId::new()).bus_name().unwrap();
        assert!(!is_plottable(&sensor_ref), "speed@<uuid> is plumbing");
    }

    #[test]
    fn the_picker_shows_everything_until_a_series_is_hidden() {
        let sensor_ref = SignalSource::Speed(StableId::new()).bus_name().unwrap();
        let names = ["speed", "warm", sensor_ref.as_str()];

        // Nothing hidden → every plottable series shows (plumbing still out).
        let none = HashSet::new();
        assert_eq!(
            visible_series(names.iter().copied(), &none),
            ["speed", "warm"]
        );

        // Hiding one drops just that series.
        let hidden: HashSet<String> = ["warm".to_owned()].into_iter().collect();
        assert_eq!(visible_series(names.iter().copied(), &hidden), ["speed"]);
    }
}
