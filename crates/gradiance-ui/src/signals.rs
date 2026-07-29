//! The **signal list**: the dataflow graph (`docs/signal-dataflow.md`) as a
//! form — params (auto-slider knobs), computed modulators, and source→sink
//! bindings.
//!
//! # Why this is not a pane
//!
//! It used to be one, sharing a right-dock tab with the plotter, which made
//! three surfaces for one model: a canvas that could wire blocks, a list that
//! could rename and delete them, and a plot of the result — none of them next
//! to each other. The list now lives **inside the node-graph pane**, beside the
//! canvas, because they are two views of the same graph: the canvas is the one
//! you draw in, the list is the one that shows names, compile errors, and the
//! edits a canvas has no gesture for (rename a binding, retarget a sink, delete
//! a param, type a script-published bus name). The plotter kept the pane.
//!
//! The signal resources (`SignalBindings`, `SignalParams`, `ComputedSignals`)
//! are config-seam (invariant-#4 class), so this edits them directly: no
//! intents, no undo, exactly like the grid and snap settings. The one exception
//! is the behavior-node editor, which edits *authored* state and therefore
//! emits a `PropertyEditIntent` like everything else.

use crate::widgets;
use bevy_egui::egui;
use gradiance_core::ids::StableId;
use gradiance_signal::{
    Curve, GradientSpec, SignalBinding, SignalBindings, SignalBus, SignalMap, SignalSink,
    SignalSource,
};

/// Everything the list reads and writes, as **borrowed pieces** rather than a
/// `SystemParam` bundle.
///
/// It used to be a bundle, and that was a mistake the ECS caught: the list now
/// lives inside the node-graph pane, whose host already holds `SignalParams`,
/// `ComputedSignals` and `SignalBindings` for the canvas. A second bundle
/// requesting the same resources is a system-parameter conflict — Bevy panics
/// at schedule build, and `tests/it/ui_conflicts.rs` fails. A leaf renderer
/// should not be deciding what the world lends it; the host does that once and
/// passes the pieces down.
pub struct SignalListView<'a> {
    /// Tunable slider params.
    pub params: &'a mut Vec<gradiance_signal::SignalParam>,
    /// Computed modulator signals.
    pub computed: &'a mut Vec<gradiance_signal::ComputedSignal>,
    /// The live bus (current values for readouts).
    pub bus: &'a SignalBus,
    /// Compile errors to surface.
    pub compiled: &'a gradiance_signal::CompiledSignals,
    /// The selected behavior node to edit, if exactly one is selected.
    pub node: Option<(StableId, gradiance_domain::node::NodeKind)>,
}

/// Renders the whole signal list into `ui`. `selected` is the current body
/// selection (for the binding add-buttons).
pub fn signals_section(
    ui: &mut egui::Ui,
    view: &mut SignalListView,
    bindings: &mut SignalBindings,
    selected: &[StableId],
    edits: &mut bevy::prelude::MessageWriter<gradiance_command::intent::PropertyEditIntent>,
) {
    egui::ScrollArea::vertical()
        .id_salt("signals-section")
        .show(ui, |ui| {
            node_block(ui, view.node.clone(), edits);
            params_block(ui, view.params);
            ui.separator();
            computed_block(ui, view.computed, view.compiled, view.bus);
            ui.separator();
            bindings_block(ui, bindings, selected);
        });
}

/// Editor for the selected behavior node — wires sensor/actuator signal
/// names (the node-graph edges) and tracer fade. Emits one undoable
/// [`PropertyEditIntent`](gradiance_command::intent::PropertyEditIntent) per
/// change (authored state → the command seam, never a direct write).
fn node_block(
    ui: &mut egui::Ui,
    node: Option<(StableId, gradiance_domain::node::NodeKind)>,
    edits: &mut bevy::prelude::MessageWriter<gradiance_command::intent::PropertyEditIntent>,
) {
    let Some((id, kind)) = node else {
        return;
    };
    ui.label(egui::RichText::new(format!("Node: {}", kind.label())).strong());
    if let Some(next) = widgets::node_kind_editor(ui, "list", &kind) {
        edits.write(gradiance_command::intent::PropertyEditIntent {
            changes: vec![gradiance_command::property::PropertyChange {
                id,
                old: gradiance_command::property::PropertyValue::NodeKind(kind),
                new: gradiance_command::property::PropertyValue::NodeKind(next),
            }],
        });
    }
    ui.separator();
}

/// The param sliders (`defparam` knobs) — the P2 driver inputs.
fn params_block(ui: &mut egui::Ui, params: &mut Vec<gradiance_signal::SignalParam>) {
    widgets::section_header(ui, "Parameters");
    let mut remove = None;
    for (i, param) in params.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut param.value, param.min..=param.max)
                    .text(&param.name)
                    .clamping(egui::SliderClamping::Never),
            );
            if widgets::close_button(ui, "delete this parameter") {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        params.remove(i);
    }
    if ui.button("+ param").clicked() {
        let name = fresh(params.iter().map(|p| p.name.as_str()), "param");
        params.push(gradiance_signal::SignalParam::unit(name));
    }
}

/// The computed-signal list (`defsignal` modulators): name, RPN expression,
/// current bus value, and any compile error.
fn computed_block(
    ui: &mut egui::Ui,
    computed: &mut Vec<gradiance_signal::ComputedSignal>,
    compiled: &gradiance_signal::CompiledSignals,
    bus: &SignalBus,
) {
    widgets::section_header(ui, "Computed");
    let mut remove = None;
    for (i, signal) in computed.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.monospace(&signal.name);
            if let Some(v) = bus.get(&signal.name) {
                ui.weak(format!("= {v:.3}"));
            }
            if widgets::close_button(ui, "delete this computed signal") {
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
    widgets::section_header(ui, "Bindings");
    add_buttons(ui, bindings, selected);
    if bindings.0.is_empty() {
        widgets::empty_state(
            ui,
            "Select a body and add a source above; its value drives the sink.",
        );
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

/// Renders a [`SignalExpr`](gradiance_signal::SignalExpr) back to its RPN form
/// (a stable, compact display; the authoring surface is the console).
fn expr_rpn(expr: &gradiance_signal::SignalExpr) -> String {
    use gradiance_signal::SignalExpr as E;
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
        // No RPN token exists for a curve (its parameter is a shape), so this
        // is a readable rendering, not a round-trip.
        E::Curve(a, _) => format!("{} curve", expr_rpn(a)),
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
        SignalSource::PosX(_) => "pos x",
        SignalSource::Distance(..) => "distance",
        SignalSource::ContactForce(_) => "contact force",
        SignalSource::ContactCount(_) => "contact count",
        SignalSource::KineticEnergy(_) => "energy",
        SignalSource::Momentum(_) => "momentum",
        SignalSource::AngularMomentum(_) => "ang mom",
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
                    curve: None,
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
        add("energy", first.map(SignalSource::KineticEnergy));
        add("momentum", first.map(SignalSource::Momentum));
        add("ang mom", first.map(SignalSource::AngularMomentum));
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
        if widgets::close_button(ui, "remove this binding") {
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
                        | SignalSource::PosX(id)
                        | SignalSource::ContactForce(id)
                        | SignalSource::ContactCount(id)
                        | SignalSource::KineticEnergy(id)
                        | SignalSource::Momentum(id)
                        | SignalSource::AngularMomentum(id)
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
    curve_section(ui, i, binding);
}

/// The optional response curve, collapsed by default: a checkbox that adds or
/// removes it, and the editor when present.
///
/// `None` and the identity curve behave identically, so the checkbox is the
/// honest control — it is the difference between "no reshaping" and "a curve I
/// am shaping", not a value change. Removing it drops the points rather than
/// flattening them, so re-enabling starts from the identity again.
fn curve_section(ui: &mut egui::Ui, i: usize, binding: &mut SignalBinding) {
    let mut enabled = binding.curve.is_some();
    if ui
        .checkbox(&mut enabled, "response curve")
        .on_hover_text("reshape the normalized value before it drives the sink")
        .changed()
    {
        binding.curve = enabled.then(Curve::default);
    }
    if let Some(curve) = binding.curve.as_mut() {
        crate::curve::curve_editor(ui, &format!("binding-curve-{i}"), curve);
    }
}
