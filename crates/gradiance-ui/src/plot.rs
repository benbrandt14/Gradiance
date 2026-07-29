//! The live plotter: a time-series panel drawn **entirely from the
//! [`SignalBus`]** (`docs/signal-dataflow.md`).
//!
//! There is one history in the system — the bus. To plot a quantity you wire
//! it to the **plot sink**: a `SignalSink::Plot` binding publishes its source
//! on the bus under its name, and this panel draws every bus signal with a
//! recorded history. The inspector's sensor ports have a one-click "plot"
//! toggle that adds/removes such a binding, so plotting a body's speed is a
//! click. Recording pauses with the simulation (the bus records only while
//! playing).
//!
//! # Why `egui_plot` rather than the painter
//!
//! This used to hand-draw each series into a fixed 70 px strip, min/max-scaled
//! to itself, with the sample index as x. That made three things impossible
//! that a plotter is *for*: you could not zoom or pan into a moment, you could
//! not read a value off the curve, and you could not compare two series
//! (each had its own invisible scale, so equal heights meant nothing).
//! [`egui_plot`] supplies axes, zoom/pan, a legend, and a cursor readout;
//! [`BusEntry::times`](gradiance_signal::BusEntry::times) supplies the real
//! time axis. What remains here is the projection from bus to plot.
//!
//! [`SignalBus`]: gradiance_signal::SignalBus

use crate::widgets;
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::{HashSet, VecDeque};

/// Plot panel visibility.
#[derive(Resource, Default)]
pub struct PlotPanel {
    open: bool,
}

/// How the visible series share vertical space.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PlotLayout {
    /// One plot, all series overlaid on a shared y axis — the comparison view.
    #[default]
    Overlay,
    /// One plot per series, stacked. Each gets its own y scale, so a series
    /// with a tiny range is still readable next to a large one.
    Stacked,
}

impl PlotLayout {
    /// The two options, in menu order.
    pub const ALL: [Self; 2] = [Self::Overlay, Self::Stacked];

    fn label(self) -> &'static str {
        match self {
            Self::Overlay => "Overlay",
            Self::Stacked => "Stacked",
        }
    }
}

/// Plotter configuration: which series are hidden (default: show all) and how
/// they share space. A signal is drawn unless the user unchecks it, so new
/// signals appear automatically. Editor view-state — never persisted.
#[derive(Resource, Default)]
pub struct PlotConfig {
    hidden: HashSet<String>,
    layout: PlotLayout,
}

crate::impl_panel_toggle!(PlotPanel, open);

/// Colours cycled across the plotted signals.
const SIGNAL_COLORS: [egui::Color32; 4] = [
    egui::Color32::from_rgb(120, 200, 255),
    egui::Color32::from_rgb(255, 190, 120),
    egui::Color32::from_rgb(150, 230, 150),
    egui::Color32::from_rgb(230, 150, 230),
];

/// One plottable series: a bus signal's recorded history with the timestamps
/// that go with it. `unit` is the SI symbol of the series' binding source
/// dimension (the P3 catalog) — empty for params, computed signals, and
/// anything without a dimensioned source.
///
/// `values` and `times` are the same length (the bus records them together),
/// but [`points`](Self::points) does not assume it.
#[derive(Clone, Copy)]
pub struct Series<'a> {
    /// The bus name, as shown in the legend and the picker.
    pub name: &'a str,
    /// SI unit symbol, or empty.
    pub unit: &'static str,
    /// Recorded samples, oldest first.
    pub values: &'a VecDeque<f32>,
    /// Simulated seconds for each sample.
    pub times: &'a VecDeque<f32>,
}

impl Series<'_> {
    /// The legend/axis label: the name, plus the unit when the series has one.
    pub fn label(&self) -> String {
        if self.unit.is_empty() {
            self.name.to_owned()
        } else {
            format!("{}  [{}]", self.name, self.unit)
        }
    }

    /// `(time, value)` pairs for the plot. Zipping is what makes a paused run
    /// read correctly: the gap in the timestamps becomes a gap on the axis
    /// instead of samples sliding left.
    fn points(&self) -> egui_plot::PlotPoints<'static> {
        self.times
            .iter()
            .zip(self.values.iter())
            .map(|(&t, &v)| [f64::from(t), f64::from(v)])
            .collect::<Vec<_>>()
            .into()
    }
}

/// Every named bus signal the plotter can draw, in bus order — params,
/// computed signals, and plot-sink bindings. The canonical sensor-ref names
/// (`speed@<uuid>`) that `publish_sensor_refs` puts on the bus for modulation
/// operands are internal plumbing, not user-chosen series, so `is_plottable`
/// drops them. The dock host computes this from the bus and hands it to
/// [`plot_section`].
pub fn plottable_series<'a>(
    bus: &'a gradiance_signal::SignalBus,
    bindings: &gradiance_domain::signal::SignalBindings,
) -> Vec<Series<'a>> {
    bus.entries()
        .filter(|(name, e)| e.history().len() >= 2 && is_plottable(name))
        .map(|(name, e)| Series {
            name,
            unit: bindings
                .0
                .iter()
                .find(|b| b.name == name)
                .map_or("", |b| b.source.dimension().symbol()),
            values: e.history(),
            times: e.times(),
        })
        .collect()
}

/// Renders the plotter's content into `ui` (the **Live Plot** dock pane): a
/// header of controls, then the visible series on a shared time axis (or one
/// plot each, stacked). `plottable` is the current [`plottable_series`]; the
/// panel's open/toggle handling lives in the dock host.
pub fn plot_section(ui: &mut egui::Ui, plottable: &[Series<'_>], config: &mut PlotConfig) {
    if plottable.is_empty() {
        widgets::empty_state(
            ui,
            "Nothing recorded yet — wire a sensor to the plot sink (a body's \
             ▸plot toggle, or a plot binding) and press Play.",
        );
        return;
    }
    let mut reset = false;
    plot_header(ui, plottable, config, &mut reset);

    let visible = visible_series(plottable, &config.hidden);
    if visible.is_empty() {
        widgets::empty_state(ui, "No series selected — pick one above.");
        return;
    }
    match config.layout {
        PlotLayout::Overlay => overlay_plot(ui, &visible, reset),
        PlotLayout::Stacked => stacked_plots(ui, &visible, reset),
    }
}

/// The series the user has not hidden, in bus order — pure, so the selection
/// logic is unit-testable without a window.
fn visible_series<'s>(plottable: &[Series<'s>], hidden: &HashSet<String>) -> Vec<Series<'s>> {
    plottable
        .iter()
        .filter(|s| !hidden.contains(s.name))
        .copied()
        .collect()
}

/// The controls strip: the series picker, the layout choice, and a reset that
/// returns the view to auto-fit after a zoom.
fn plot_header(
    ui: &mut egui::Ui,
    plottable: &[Series<'_>],
    config: &mut PlotConfig,
    reset: &mut bool,
) {
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Series", |ui| {
            for series in plottable {
                let mut shown = !config.hidden.contains(series.name);
                if ui.checkbox(&mut shown, series.label()).changed() {
                    if shown {
                        config.hidden.remove(series.name);
                    } else {
                        config.hidden.insert(series.name.to_owned());
                    }
                }
            }
        })
        .response
        .on_hover_text("show or hide individual series");

        for layout in PlotLayout::ALL {
            if ui
                .selectable_label(config.layout == layout, layout.label())
                .clicked()
            {
                config.layout = layout;
            }
        }
        if ui
            .small_button(crate::fonts::glyph::FIT)
            .on_hover_text("reset zoom — back to auto-fit")
            .clicked()
        {
            *reset = true;
        }
    });
}

/// All visible series on one shared-axis plot — the comparison view.
fn overlay_plot(ui: &mut egui::Ui, visible: &[Series<'_>], reset: bool) {
    let height = ui.available_height().max(MIN_PLOT_HEIGHT);
    base_plot("plot-overlay", height)
        .legend(egui_plot::Legend::default())
        .show(ui, |plot_ui| {
            if reset {
                plot_ui.set_auto_bounds(true);
            }
            for (i, series) in visible.iter().enumerate() {
                plot_ui.line(
                    egui_plot::Line::new(series.label(), series.points())
                        .color(SIGNAL_COLORS[i % SIGNAL_COLORS.len()])
                        .width(1.5),
                );
            }
        });
}

/// One plot per series, each auto-scaled to its own range. Linked on x so
/// zooming one moves all of them — the axis they share is time.
fn stacked_plots(ui: &mut egui::Ui, visible: &[Series<'_>], reset: bool) {
    let each = (ui.available_height() / visible.len() as f32).max(MIN_PLOT_HEIGHT);
    for (i, series) in visible.iter().enumerate() {
        ui.label(egui::RichText::new(series.label()).small().weak());
        base_plot(format!("plot-{}", series.name), each)
            // Share the time axis (x only) and the cursor across the stack, so
            // zooming or hovering one series lines up with the rest.
            .link_axis("plot-time-axis", egui::Vec2b::new(true, false))
            .link_cursor("plot-time-axis", egui::Vec2b::new(true, false))
            .show(ui, |plot_ui| {
                if reset {
                    plot_ui.set_auto_bounds(true);
                }
                plot_ui.line(
                    egui_plot::Line::new(series.label(), series.points())
                        .color(SIGNAL_COLORS[i % SIGNAL_COLORS.len()])
                        .width(1.5),
                );
            });
    }
}

/// The shared plot configuration: a seconds-labelled x axis, a cursor readout,
/// and no y-axis drag (vertical scale is auto — dragging it fights the live
/// data, which keeps growing).
fn base_plot(id: impl egui::AsId, height: f32) -> egui_plot::Plot<'static> {
    egui_plot::Plot::new(id)
        .height(height)
        .allow_scroll(false)
        .x_axis_formatter(|mark, _| format!("{:.2} s", mark.value))
        .label_formatter(|hover| {
            Some(match hover {
                egui_plot::HoverPosition::NearDataPoint {
                    plot_name,
                    position,
                    ..
                } => format!("{plot_name}\n{:.2} s   {:.4}", position.x, position.y),
                egui_plot::HoverPosition::Elsewhere { position } => {
                    format!("{:.2} s   {:.4}", position.x, position.y)
                }
            })
        })
}

/// Floor for a plot's height, so a stack of many series stays readable
/// (the pane scrolls rather than squashing them into slivers).
const MIN_PLOT_HEIGHT: f32 = 80.0;

/// Whether a bus signal is a user-facing series the plot should draw. Params,
/// computed signals, and plot-sink bindings qualify; the canonical sensor-ref
/// names (`speed@<uuid>`) `publish_sensor_refs` surfaces for modulation
/// operands are internal plumbing and stay out of the plot.
fn is_plottable(name: &str) -> bool {
    gradiance_signal::SignalSource::from_bus_name(name).is_none()
}

#[cfg(test)]
mod tests {
    use super::{Series, is_plottable, visible_series};
    use gradiance_core::ids::StableId;
    use gradiance_signal::SignalSource;
    use std::collections::{HashSet, VecDeque};

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
        let values: VecDeque<f32> = [1.0, 2.0].into_iter().collect();
        let times: VecDeque<f32> = [0.0, 0.1].into_iter().collect();
        let series = |name| Series {
            name,
            unit: "",
            values: &values,
            times: &times,
        };
        let all = [series("speed"), series("warm")];

        // Nothing hidden → every series shows.
        let none = HashSet::new();
        let names: Vec<&str> = visible_series(&all, &none).iter().map(|s| s.name).collect();
        assert_eq!(names, ["speed", "warm"]);

        // Hiding one drops just that series.
        let hidden: HashSet<String> = ["warm".to_owned()].into_iter().collect();
        let names: Vec<&str> = visible_series(&all, &hidden)
            .iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["speed"]);
    }

    /// The point of carrying timestamps: a run that was paused leaves a gap in
    /// the stamps, and the plot must show that gap rather than sliding the
    /// later samples left onto a dense sample-index axis.
    #[test]
    fn points_pair_each_sample_with_its_own_timestamp() {
        let values: VecDeque<f32> = [1.0, 2.0, 3.0].into_iter().collect();
        // Paused between the second and third sample: 0.0, 0.1, then 5.0.
        let times: VecDeque<f32> = [0.0, 0.1, 5.0].into_iter().collect();
        let series = Series {
            name: "speed",
            unit: "m/s",
            values: &values,
            times: &times,
        };
        let points = series.points().points().to_vec();
        assert_eq!(points.len(), 3);
        assert!((points[2].x - 5.0).abs() < 1e-9, "the pause is visible");
        assert!((points[2].y - 3.0).abs() < 1e-9);
        assert_eq!(series.label(), "speed  [m/s]", "unit is appended");
    }

    /// Ragged input must not panic or invent samples — `zip` stops at the
    /// shorter of the two, which is the only safe reading.
    #[test]
    fn a_series_with_fewer_stamps_than_values_is_truncated_not_guessed() {
        let values: VecDeque<f32> = [1.0, 2.0, 3.0].into_iter().collect();
        let times: VecDeque<f32> = [0.0, 0.1].into_iter().collect();
        let series = Series {
            name: "x",
            unit: "",
            values: &values,
            times: &times,
        };
        assert_eq!(series.points().points().len(), 2);
        assert_eq!(series.label(), "x", "no unit, no brackets");
    }
}
