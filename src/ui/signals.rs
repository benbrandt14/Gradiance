//! The Signals **dock section**: plain, functional editing of the
//! signal-dataflow graph (`docs/signal-dataflow.md`) — params (auto-slider
//! knobs), computed modulators, and source→sink bindings.
//!
//! Deliberately visuals-light — sliders and rows — while the dataflow
//! substrate matures; the node-editor canvas replaces this surface later
//! (drag a property out of a panel → a source node). The signal resources
//! (`SignalBindings`, `SignalParams`, `ComputedSignals`) are config-seam
//! (invariant-#4 class), so the UI edits them directly: no intents, no
//! undo, exactly like the grid and snap settings. Lives in the right dock
//! (`ui::dock`), the direction toward dockable inspectors.

use crate::core::ids::StableId;
use crate::signal::{
    ComputedSignals, GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap,
    SignalParams, SignalSink, SignalSource,
};
use bevy::ecs::system::SystemParam;
use bevy_egui::egui;

/// Signals dock section visibility.
#[derive(bevy::prelude::Resource, Default)]
pub struct SignalsPanel {
    open: bool,
}

impl SignalsPanel {
    /// Whether the section is shown (read by the transport toggle + dock).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Flips the section's visibility.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// The signal-graph config resources the dock section edits, bundled so the
/// dock host stays under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct SignalsDock<'w, 's> {
    /// Source→sink bindings.
    pub bindings: bevy::prelude::ResMut<'w, SignalBindings>,
    /// Tunable slider params.
    pub params: bevy::prelude::ResMut<'w, SignalParams>,
    /// Computed modulator signals.
    pub computed: bevy::prelude::ResMut<'w, ComputedSignals>,
    /// The live bus (read-only: current values for readouts).
    pub bus: bevy::prelude::Res<'w, SignalBus>,
    /// Compile errors to surface (read-only).
    pub compiled: bevy::prelude::Res<'w, crate::signal::CompiledSignals>,
    /// Selected behavior nodes to edit (id + kind).
    pub nodes: bevy::prelude::Query<
        'w,
        's,
        (
            bevy::prelude::Entity,
            &'static StableId,
            &'static crate::domain::node::NodeKind,
        ),
        bevy::prelude::With<crate::domain::node::BehaviorNode>,
    >,
    /// Undoable node edits (wiring sensor/actuator signal names).
    pub edits: bevy::prelude::MessageWriter<'w, crate::command::intent::PropertyEditIntent>,
}

/// Renders the whole Signals section into a dock `ui`. `selected` is the
/// current body selection (for the binding add-buttons); `selection` is the
/// full selection (to find a selected node to edit).
pub fn signals_section(
    ui: &mut egui::Ui,
    dock: &mut SignalsDock,
    selected: &[StableId],
    selection: &crate::interaction::selection::Selection,
) {
    egui::ScrollArea::vertical()
        .id_salt("signals-section")
        .show(ui, |ui| {
            node_block(ui, dock, selection);
            params_block(ui, &mut dock.params.0);
            ui.separator();
            computed_block(ui, &mut dock.computed.0, &dock.compiled, &dock.bus);
            ui.separator();
            bindings_block(ui, &mut dock.bindings, selected);
        });
}

/// Editor for the selected behavior node — wires sensor/actuator signal
/// names (the node-graph edges) and tracer fade. Emits one undoable
/// [`PropertyEditIntent`](crate::command::intent::PropertyEditIntent) per
/// change (authored state → the command seam, never a direct write).
fn node_block(
    ui: &mut egui::Ui,
    dock: &mut SignalsDock,
    selection: &crate::interaction::selection::Selection,
) {
    let Some((id, kind)) = selection
        .iter()
        .find_map(|e| dock.nodes.get(e).ok().map(|(_, id, k)| (*id, k.clone())))
    else {
        return;
    };
    ui.label(egui::RichText::new(format!("Node: {}", kind.label())).strong());
    if let Some(next) = node_kind_editor(ui, "dock", &kind) {
        dock.edits
            .write(crate::command::intent::PropertyEditIntent {
                changes: vec![crate::command::property::PropertyChange {
                    id,
                    old: crate::command::property::PropertyValue::NodeKind(kind),
                    new: crate::command::property::PropertyValue::NodeKind(next),
                }],
            });
    }
    ui.separator();
}

/// The behavior-node kind editor widgets — shared by the Signals dock and
/// the node context menu. Returns the edited kind when a field changed
/// (the caller emits one undoable `PropertyEditIntent`). `salt` keeps
/// widget ids unique between host surfaces.
pub fn node_kind_editor(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &crate::domain::node::NodeKind,
) -> Option<crate::domain::node::NodeKind> {
    use crate::domain::node::{NodeKind, SensorQuantity};
    let mut next = kind.clone();
    match &mut next {
        NodeKind::Tracer(tracer) => {
            ui.horizontal(|ui| {
                ui.label("fade (s)");
                ui.add(egui::DragValue::new(&mut tracer.fade_secs).speed(0.05));
                ui.label("size");
                ui.add(
                    egui::DragValue::new(&mut tracer.size)
                        .speed(0.1)
                        .range(0.5..=20.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("pattern");
                egui::ComboBox::from_id_salt(ui.id().with((salt, "pattern")))
                    .selected_text(format!("{:?}", tracer.pattern))
                    .show_ui(ui, |ui| {
                        for p in crate::domain::tracer::TracePattern::ALL {
                            ui.selectable_value(&mut tracer.pattern, p, format!("{p:?}"));
                        }
                    });
            });
        }
        NodeKind::Sensor { quantity, signal } => {
            ui.horizontal(|ui| {
                ui.label("reads");
                egui::ComboBox::from_id_salt(ui.id().with((salt, "q")))
                    .selected_text(quantity.label())
                    .show_ui(ui, |ui| {
                        for q in SensorQuantity::ALL {
                            ui.selectable_value(quantity, q, q.label());
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("→ signal");
                ui.text_edit_singleline(signal);
            });
        }
        NodeKind::Actuator {
            signal,
            map,
            gradient,
            target,
        } => {
            ui.horizontal(|ui| {
                ui.label("signal →");
                ui.text_edit_singleline(signal);
            });
            ui.horizontal(|ui| {
                ui.label("domain");
                ui.add(egui::DragValue::new(&mut map.in_min).speed(1.0));
                ui.label("→");
                ui.add(egui::DragValue::new(&mut map.in_max).speed(1.0));
                egui::ComboBox::from_id_salt(ui.id().with((salt, "grad")))
                    .selected_text(format!("{gradient:?}"))
                    .show_ui(ui, |ui| {
                        for g in GradientSpec::ALL {
                            ui.selectable_value(gradient, g, format!("{g:?}"));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("drives");
                egui::ComboBox::from_id_salt(ui.id().with((salt, "target")))
                    .selected_text(target.label())
                    .show_ui(ui, |ui| {
                        for t in crate::domain::node::ActuatorTarget::ALL {
                            ui.selectable_value(target, t, t.label());
                        }
                    });
            });
        }
    }
    (next != *kind).then_some(next)
}

/// The param sliders (`defparam` knobs) — the P2 driver inputs.
fn params_block(ui: &mut egui::Ui, params: &mut Vec<crate::signal::SignalParam>) {
    ui.label(egui::RichText::new("Parameters").strong());
    let mut remove = None;
    for (i, param) in params.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut param.value, param.min..=param.max)
                    .text(&param.name)
                    .clamping(egui::SliderClamping::Never),
            );
            if ui.small_button("✖").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        params.remove(i);
    }
    if ui.button("+ param").clicked() {
        let name = fresh(params.iter().map(|p| p.name.as_str()), "param");
        params.push(crate::signal::SignalParam::unit(name));
    }
}

/// The computed-signal list (`defsignal` modulators): name, RPN expression,
/// current bus value, and any compile error.
fn computed_block(
    ui: &mut egui::Ui,
    computed: &mut Vec<crate::signal::ComputedSignal>,
    compiled: &crate::signal::CompiledSignals,
    bus: &SignalBus,
) {
    ui.label(egui::RichText::new("Computed").strong());
    let mut remove = None;
    for (i, signal) in computed.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.monospace(&signal.name);
            if let Some(v) = bus.get(&signal.name) {
                ui.weak(format!("= {v:.3}"));
            }
            if ui.small_button("✖").clicked() {
                remove = Some(i);
            }
        });
        // The expression, rendered back to RPN; editing is via the console
        // `(defsignal …)` for now — the node canvas replaces this later.
        ui.monospace(egui::RichText::new(expr_rpn(&signal.expr)).weak());
        if let Some((_, err)) = compiled.errors.iter().find(|(n, _)| *n == signal.name) {
            ui.colored_label(egui::Color32::from_rgb(220, 120, 120), err);
        }
    }
    if let Some(i) = remove {
        computed.remove(i);
    }
    if computed.is_empty() {
        ui.weak("Add via console: (defsignal \"osc\" \"t sin amp *\")");
    }
}

/// The source→sink bindings (unchanged model).
fn bindings_block(ui: &mut egui::Ui, bindings: &mut SignalBindings, selected: &[StableId]) {
    ui.label(egui::RichText::new("Bindings").strong());
    add_buttons(ui, bindings, selected);
    if bindings.0.is_empty() {
        ui.weak("Select a body and add a source above; its value drives the sink.");
    }
    let mut remove = None;
    for (i, binding) in bindings.0.iter_mut().enumerate() {
        ui.separator();
        binding_row(ui, i, binding, &mut remove);
    }
    if let Some(i) = remove {
        bindings.0.remove(i);
    }
}

/// Renders a [`SignalExpr`](crate::signal::SignalExpr) back to its RPN form
/// (a stable, compact display; the authoring surface is the console).
fn expr_rpn(expr: &crate::signal::SignalExpr) -> String {
    use crate::signal::SignalExpr as E;
    match expr {
        E::Const(c) => format!("{c}"),
        E::Input(name) => name.clone(),
        E::Neg(a) => format!("{} neg", expr_rpn(a)),
        E::Sin(a) => format!("{} sin", expr_rpn(a)),
        E::Cos(a) => format!("{} cos", expr_rpn(a)),
        E::Abs(a) => format!("{} abs", expr_rpn(a)),
        E::Add(a, b) => format!("{} {} +", expr_rpn(a), expr_rpn(b)),
        E::Sub(a, b) => format!("{} {} -", expr_rpn(a), expr_rpn(b)),
        E::Mul(a, b) => format!("{} {} *", expr_rpn(a), expr_rpn(b)),
        E::Div(a, b) => format!("{} {} /", expr_rpn(a), expr_rpn(b)),
        E::Min(a, b) => format!("{} {} min", expr_rpn(a), expr_rpn(b)),
        E::Max(a, b) => format!("{} {} max", expr_rpn(a), expr_rpn(b)),
    }
}

/// A fresh unique name over an existing set.
fn fresh<'a>(existing: impl Iterator<Item = &'a str>, base: &str) -> String {
    let taken: Vec<&str> = existing.collect();
    let mut n = 1;
    loop {
        let name = format!("{base}-{n}");
        if !taken.iter().any(|t| *t == name) {
            return name;
        }
        n += 1;
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
                let name = fresh(bindings.0.iter().map(|b| b.name.as_str()), label);
                bindings.0.push(SignalBinding {
                    name,
                    source,
                    map: SignalMap::default(),
                    gradient: GradientSpec::default(),
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
