//! The Signals window: plain, functional editing of the signal-dataflow
//! bindings (`docs/signal-dataflow.md`).
//!
//! Deliberately visuals-light — a row per binding with combos and drags —
//! while the dataflow substrate matures; the node-editor canvas replaces
//! this surface later (drag a property out of a panel → a source node).
//! [`SignalBindings`] is a config-seam resource (invariant-#4 class), so
//! the UI edits it directly: no intents, no undo, exactly like the grid
//! and snap settings.

use crate::core::ids::StableId;
use crate::interaction::selection::Selection;
use crate::signal::{GradientSpec, SignalBinding, SignalBindings, SignalSink, SignalSource};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Signals window visibility.
#[derive(Resource, Default)]
pub struct SignalsPanel {
    open: bool,
}

impl SignalsPanel {
    /// Whether the window is shown (read by the transport toggle).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the window's visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// A short human label for a source (the row header).
fn source_label(source: &SignalSource) -> &'static str {
    match source {
        SignalSource::Speed(_) => "speed",
        SignalSource::Spin(_) => "spin",
        SignalSource::Height(_) => "height",
        SignalSource::Distance(..) => "distance",
        SignalSource::ContactForce(_) => "contact force",
        SignalSource::ContactCount(_) => "contact count",
        SignalSource::Named(_) => "named",
    }
}

/// A unique bus name for a new binding.
fn fresh_name(bindings: &SignalBindings, base: &str) -> String {
    let mut n = 1;
    loop {
        let name = format!("{base}-{n}");
        if !bindings.0.iter().any(|b| b.name == name) {
            return name;
        }
        n += 1;
    }
}

/// Renders the Signals window: add-from-selection buttons + one editable
/// row per binding.
pub fn signals_panel(
    mut contexts: EguiContexts,
    mut panel: ResMut<SignalsPanel>,
    mut bindings: ResMut<SignalBindings>,
    selection: Res<Selection>,
    ids: Query<&StableId>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    if !panel.open {
        return Ok(());
    }
    let selected: Vec<StableId> = selection
        .iter()
        .filter_map(|e| ids.get(e).ok().copied())
        .collect();

    let mut open = true;
    egui::Window::new("Signals")
        .open(&mut open)
        .default_width(340.0)
        .show(ctx, |ui| {
            add_buttons(ui, &mut bindings, &selected);
            if bindings.0.is_empty() {
                ui.label("Select a body and add a source above; its value drives the sink.");
            }
            let mut remove = None;
            for (i, binding) in bindings.0.iter_mut().enumerate() {
                ui.separator();
                binding_row(ui, i, binding, &mut remove);
            }
            if let Some(i) = remove {
                bindings.0.remove(i);
            }
        });
    panel.open = open;
    Ok(())
}

/// The "+ source" buttons — each wires the current selection into a new
/// binding (source and sink default to the first selected body).
fn add_buttons(ui: &mut egui::Ui, bindings: &mut SignalBindings, selected: &[StableId]) {
    let first = selected.first().copied();
    ui.horizontal_wrapped(|ui| {
        ui.label("add:");
        let mut add = |label: &str, source: Option<SignalSource>| {
            if ui
                .add_enabled(source.is_some(), egui::Button::new(label))
                .clicked()
                && let (Some(source), Some(target)) = (source, first)
            {
                let name = fresh_name(bindings, label);
                bindings.0.push(SignalBinding {
                    name,
                    source,
                    map: default(),
                    gradient: default(),
                    sink: SignalSink::Fill(target),
                });
            }
        };
        add("speed", first.map(SignalSource::Speed));
        add("spin", first.map(SignalSource::Spin));
        add("height", first.map(SignalSource::Height));
        add("contact force", first.map(SignalSource::ContactForce));
        add("contact count", first.map(SignalSource::ContactCount));
        add(
            "distance",
            match selected {
                [a, b, ..] => Some(SignalSource::Distance(*a, *b)),
                _ => None,
            },
        );
        add("named", Some(SignalSource::Named(String::new())));
    });
}

/// One binding's editable row.
fn binding_row(
    ui: &mut egui::Ui,
    i: usize,
    binding: &mut SignalBinding,
    remove: &mut Option<usize>,
) {
    ui.horizontal(|ui| {
        ui.strong(source_label(&binding.source));
        ui.text_edit_singleline(&mut binding.name);
        if ui.small_button("✖").clicked() {
            *remove = Some(i);
        }
    });
    if let SignalSource::Named(name) = &mut binding.source {
        ui.horizontal(|ui| {
            ui.label("bus signal");
            ui.text_edit_singleline(name)
                .on_hover_text("a name published by a script via (signal-set name value)");
        });
    }
    ui.horizontal(|ui| {
        ui.label("domain");
        ui.add(egui::DragValue::new(&mut binding.map.in_min).speed(1.0));
        ui.label("→");
        ui.add(egui::DragValue::new(&mut binding.map.in_max).speed(1.0));
        egui::ComboBox::from_id_salt(ui.id().with(("gradient", i)))
            .selected_text(format!("{:?}", binding.gradient))
            .show_ui(ui, |ui| {
                for option in GradientSpec::ALL {
                    ui.selectable_value(&mut binding.gradient, option, format!("{option:?}"));
                }
            });
        let sink_label = match binding.sink {
            SignalSink::Fill(_) => "→ fill",
            SignalSink::TracerColor(_) => "→ tracer",
            SignalSink::Plot => "→ plot",
        };
        egui::ComboBox::from_id_salt(ui.id().with(("sink", i)))
            .selected_text(sink_label)
            .show_ui(ui, |ui| {
                // Re-targeting keeps the sink's body: reuse the current one
                // (falling back to the source's body for Plot → color flips).
                let target = match &binding.sink {
                    SignalSink::Fill(id) | SignalSink::TracerColor(id) => Some(*id),
                    SignalSink::Plot => match &binding.source {
                        SignalSource::Speed(id)
                        | SignalSource::Spin(id)
                        | SignalSource::Height(id)
                        | SignalSource::ContactForce(id)
                        | SignalSource::ContactCount(id)
                        | SignalSource::Distance(id, _) => Some(*id),
                        SignalSource::Named(_) => None,
                    },
                };
                if let Some(target) = target {
                    ui.selectable_value(&mut binding.sink, SignalSink::Fill(target), "fill");
                    ui.selectable_value(
                        &mut binding.sink,
                        SignalSink::TracerColor(target),
                        "tracer",
                    );
                }
                ui.selectable_value(&mut binding.sink, SignalSink::Plot, "plot");
            });
    });
}
