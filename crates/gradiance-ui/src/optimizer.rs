//! The **Optimizer**: start a close-packing run and dial its rules.
//!
//! Three surfaces, sized to how much the user wants to think about it:
//!
//! - The selection **context menu** — one click to pack, plus accept/cancel
//!   while a run is live. The fast path.
//! - The **Properties pane** — a compact status strip and the same buttons,
//!   so a run is visible and controllable without opening anything.
//! - A floating **Optimizer window** — the full rulebook, in its own
//!   scrolling window.
//!
//! The rulebook lives in a window rather than in the Properties pane for a
//! plain reason: it is far too tall. Nested inside a dock pane it pushed the
//! per-body sections off-screen and needed its own scrollbar inside another
//! scrollbar. It is also not a per-body property — it describes an operation
//! over the whole selection — so a modeless window that can sit open next to
//! the viewport while you watch the ghost is the honest shape for it.
//!
//! All three write the same two request messages — the UI never touches the
//! session or the world, it only asks.
//!
//! Every control here edits [`PackConfig`], which is a **settings resource**:
//! editor configuration rather than authored scene state, so writing it
//! directly is the sanctioned Config seam (see `CLAUDE.md`), not a bypass of
//! the command discipline. The arrangement the solver produces still lands
//! through the ordinary intent path as one undoable command.

use crate::widgets;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_interaction::pack::{PackControl, PackSession, StartPackRequest};
use gradiance_interaction::selection::Selection;
use gradiance_optimize::{
    Boundary, EdgeAlignment, LayerRule, Objective, ObjectiveWeights, PackConfig, RotationMode,
    ShelfOrder, SolverKind,
};

/// Everything the Optimizer panel reads and writes, bundled so both hosts
/// stay under Bevy's system-parameter limit.
#[derive(SystemParam)]
pub struct OptimizerPanel<'w> {
    /// The rulebook (Config seam — UI edits this directly).
    pub config: ResMut<'w, PackConfig>,
    /// The live run, read-only here.
    pub session: Res<'w, PackSession>,
    start: MessageWriter<'w, StartPackRequest>,
    control: MessageWriter<'w, PackControl>,
    /// Whether the full rulebook is expanded in the Properties pane.
    pub expanded: ResMut<'w, OptimizerExpanded>,
}

/// Whether the floating Optimizer window is showing.
#[derive(Resource, Default, Debug)]
pub struct OptimizerExpanded(pub bool);

crate::impl_panel_toggle!(OptimizerExpanded, 0);

impl OptimizerPanel<'_> {
    /// Asks for a run over the current selection.
    pub fn request_pack(&mut self) {
        self.start.write(StartPackRequest);
    }

    /// Applies the best layout found so far.
    pub fn apply(&mut self) {
        self.control.write(PackControl::Apply);
    }

    /// Throws the run away.
    pub fn cancel(&mut self) {
        self.control.write(PackControl::Cancel);
    }
}

/// The compact context-menu entry: one button plus, while a run is live, its
/// progress and the accept/cancel pair.
///
/// Returns true when the menu should close.
pub fn context_menu_entry(
    ui: &mut egui::Ui,
    selection: &Selection,
    opt: &mut OptimizerPanel,
) -> bool {
    let mut close = false;
    ui.label(egui::RichText::new("Optimize").weak());

    if opt.session.is_active() {
        run_readout(ui, opt);
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                opt.apply();
                close = true;
            }
            if ui.button("Cancel").clicked() {
                opt.cancel();
                close = true;
            }
        });
        return close;
    }

    let enabled = selection.len() >= 2;
    if ui
        .add_enabled(enabled, egui::Button::new("Pack selection"))
        .on_hover_text(
            "Rearrange the selection to fit into the smallest area without \
             collisions. Bodies on different depth bands may share a footprint.",
        )
        .on_disabled_hover_text("Select at least two bodies to pack them")
        .clicked()
    {
        opt.request_pack();
        close = true;
    }
    if ui
        .add_enabled(enabled, egui::Button::new("⚙ Pack options…"))
        .on_hover_text("open the Optimizer window with the full rulebook")
        .clicked()
    {
        opt.expanded.0 = true;
        close = true;
    }
    close
}

/// The Properties-pane strip: status, the run buttons, and a way into the
/// full rulebook. Deliberately short — the pane has per-body sections above
/// it that must stay reachable.
pub fn optimizer_section(ui: &mut egui::Ui, selection: &Selection, opt: &mut OptimizerPanel) {
    ui.horizontal(|ui| {
        widgets::section_header(ui, "Optimizer");
        if ui
            .small_button("⚙ options…")
            .on_hover_text("open the full rulebook in its own window")
            .clicked()
        {
            opt.expanded.0 = true;
        }
    });
    run_controls(ui, selection, opt);
}

/// The floating window: the whole rulebook, scrolling.
pub fn optimizer_window(
    mut contexts: bevy_egui::EguiContexts,
    selection: Res<Selection>,
    mut opt: OptimizerPanel,
) -> Result {
    if !opt.expanded.0 {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let mut open = true;
    egui::Window::new("Optimizer")
        .open(&mut open)
        .default_width(340.0)
        .default_height(520.0)
        .resizable(true)
        .vscroll(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| optimizer_body(ui, &selection, &mut opt));
        });
    if !open {
        opt.expanded.0 = false;
    }
    Ok(())
}

/// The window contents.
fn optimizer_body(ui: &mut egui::Ui, selection: &Selection, opt: &mut OptimizerPanel) {
    run_controls(ui, selection, opt);
    ui.separator();
    preset_controls(ui, opt);
    ui.separator();
    solver_controls(ui, opt);
    ui.separator();
    goal_controls(ui, opt);
    ui.separator();
    constraint_controls(ui, opt);
    ui.separator();
    convergence_controls(ui, opt);
    ui.separator();
    tuning_controls(ui, opt);
}

/// One-click weight presets — the fastest way to say what kind of
/// arrangement is wanted without reading five sliders.
fn preset_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    ui.horizontal(|ui| {
        ui.label("preset");
        for (name, weights) in ObjectiveWeights::PRESETS {
            let active = opt.config.weights == weights;
            if ui
                .selectable_label(active, name)
                .on_hover_text(match name {
                    "Tight" => "maximum density; bodies pull into flush contact",
                    "Tidy" => "dense, but squared up — edges line up with each other",
                    _ => "spread out inside the container, touching as little as possible",
                })
                .clicked()
            {
                opt.config.weights = weights;
                // Spreading only means anything against a wall to spread
                // against; without one the set would simply fly apart.
                if name == "Spaced" && !opt.config.boundary.is_hard() {
                    opt.config.boundary = Boundary::Rect {
                        width: 8.0,
                        height: 8.0,
                    };
                }
            }
        }
    });
}

/// The objective weights — what "better" means for this run.
fn goal_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    widgets::section_header(ui, "Goal");
    let mut w = opt.config.weights;
    let mut changed = false;
    egui::Grid::new("pack-weights")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            changed |= widgets::labelled_drag(
                ui,
                "shrink",
                &mut w.extent,
                0.0..=10.0,
                0.05,
                "make the overall bounding measure smaller",
            );
            changed |= widgets::labelled_drag(
                ui,
                "fill",
                &mut w.fill,
                0.0..=10.0,
                0.05,
                "drive the filled-area ÷ bounding-area ratio toward 1",
            );
            changed |= widgets::labelled_drag(
                ui,
                "close gaps",
                &mut w.gap,
                -5.0..=10.0,
                0.05,
                "squeeze the leftover space between neighbours (negative pushes them apart)",
            );
            changed |= widgets::labelled_drag(
                ui,
                "line up",
                &mut w.parallel,
                0.0..=10.0,
                0.05,
                "reward edges pointing the same way — turns a dense pile into a tidy block",
            );
            changed |= widgets::labelled_drag(
                ui,
                "contact",
                &mut w.contact,
                -5.0..=10.0,
                0.05,
                "positive: minimize how much bodies touch. negative: pull them flush",
            );
        });
    if changed {
        opt.config.weights = w;
    }

    let mut alignment = opt.config.alignment;
    egui::ComboBox::from_id_salt("pack-alignment")
        .selected_text(alignment.label())
        .show_ui(ui, |ui| {
            for mode in EdgeAlignment::ALL {
                ui.selectable_value(&mut alignment, mode, mode.label());
            }
        });
    if alignment != opt.config.alignment {
        opt.config.alignment = alignment;
    }

    ui.horizontal(|ui| {
        let mut neighbors = opt.config.gap_neighbors;
        if ui
            .add(
                egui::DragValue::new(&mut neighbors)
                    .speed(0.2)
                    .range(1..=32)
                    .prefix("gap neighbours "),
            )
            .on_hover_text(
                "how many nearest neighbours the gap term measures against. A count, \
                 not a radius — a radius can be escaped by spreading out.",
            )
            .changed()
        {
            opt.config.gap_neighbors = neighbors;
        }
        let mut neighborhood = opt.config.neighborhood;
        if ui
            .add(
                egui::DragValue::new(&mut neighborhood)
                    .speed(0.05)
                    .range(0.0..=10.0)
                    .prefix("contact reach ×"),
            )
            .on_hover_text("how far the contact term looks, as a multiple of the mean body size")
            .changed()
        {
            opt.config.neighborhood = neighborhood;
        }
    });
}

/// Start/apply/cancel plus the live readout.
fn run_controls(ui: &mut egui::Ui, selection: &Selection, opt: &mut OptimizerPanel) {
    if opt.session.is_active() {
        run_readout(ui, opt);
        ui.horizontal(|ui| {
            if ui
                .button("Apply")
                .on_hover_text("commit the best arrangement as one undo step")
                .clicked()
            {
                opt.apply();
            }
            if ui
                .button("Cancel")
                .on_hover_text("discard the run (Escape does the same)")
                .clicked()
            {
                opt.cancel();
            }
            if ui
                .button("↻ Restart")
                .on_hover_text("re-gather the selection and solve again with the current settings")
                .clicked()
            {
                opt.request_pack();
            }
        });
        return;
    }

    let enabled = selection.len() >= 2;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled, egui::Button::new("Pack selection"))
            .on_disabled_hover_text("select at least two bodies")
            .clicked()
        {
            opt.request_pack();
        }
        let mut auto = opt.config.auto_apply;
        if ui
            .checkbox(&mut auto, "apply on convergence")
            .on_hover_text("off = hold the result as a ghost until you confirm")
            .changed()
        {
            opt.config.auto_apply = auto;
        }
    });
}

/// The live progress block.
fn run_readout(ui: &mut egui::Ui, opt: &OptimizerPanel) {
    let Some(report) = opt.session.report() else {
        return;
    };
    ui.horizontal(|ui| {
        widgets::section_header(ui, report.solver);
        ui.label(egui::RichText::new(report.status.label()).weak());
    });
    ui.add(
        egui::ProgressBar::new(report.progress())
            .desired_height(6.0)
            .text(format!("{} iter", report.total_iterations)),
    );
    let unit = if matches!(opt.config.objective, Objective::HullPerimeter) {
        "m"
    } else {
        "m²"
    };
    egui::Grid::new("pack-readout")
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            ui.label("extent");
            ui.label(format!(
                "{:.3} {unit}  ({:+.1} %)",
                report.best.extent,
                -report.shrinkage() * 100.0
            ));
            ui.end_row();
            ui.label("fill");
            ui.label(egui::RichText::new(format!("{:.1} %", report.best.fill * 100.0)).strong())
                .on_hover_text("body area ÷ bounding area — 1.0 is a perfect tiling");
            ui.end_row();
            ui.label("hull fill");
            ui.label(format!("{:.1} %", report.best.hull_fill * 100.0));
            ui.end_row();
            ui.label("gaps");
            ui.label(format!(
                "min {:.3} m, mean {:.3} m",
                report.best.min_gap, report.best.mean_gap
            ))
            .on_hover_text("leftover space to each body's nearest neighbours");
            ui.end_row();
            ui.label("aligned");
            ui.label(format!("{:.0} %", report.best.alignment * 100.0))
                .on_hover_text("how nearly the hull edges share a direction");
            ui.end_row();
            ui.label("contact");
            ui.label(format!("{:.2} m", report.best.contact));
            ui.end_row();
            ui.label("overlap");
            if report.best.is_feasible() {
                ui.label(egui::RichText::new("none").weak());
            } else {
                ui.label(
                    egui::RichText::new(format!(
                        "{:.4} m over {} pairs",
                        report.best.overlap, report.best.violations
                    ))
                    .color(egui::Color32::from_rgb(220, 140, 60)),
                );
            }
            ui.end_row();
            ui.label("bodies");
            ui.label(format!("{}", opt.session.body_count()));
            ui.end_row();
        });
}

/// Solver choice and objective.
fn solver_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    widgets::section_header(ui, "Solver");
    let mut solver = opt.config.solver;
    egui::ComboBox::from_id_salt("pack-solver")
        .selected_text(solver.label())
        .show_ui(ui, |ui| {
            for kind in SolverKind::ALL {
                ui.selectable_value(&mut solver, kind, kind.label())
                    .on_hover_text(kind.describe());
            }
        });
    if solver != opt.config.solver {
        opt.config.solver = solver;
    }
    ui.label(egui::RichText::new(solver.describe()).weak().small());

    let mut objective = opt.config.objective;
    egui::ComboBox::from_id_salt("pack-objective")
        .selected_text(objective.label())
        .show_ui(ui, |ui| {
            for kind in Objective::ALL {
                ui.selectable_value(&mut objective, kind, kind.label());
            }
        });
    if objective != opt.config.objective {
        opt.config.objective = objective;
    }
    ui.label(
        egui::RichText::new("what \"smallest\" is measured by")
            .weak()
            .small(),
    );
}

/// The problem constraints: clearance, rotation, layers, boundary, groups.
fn constraint_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    widgets::section_header(ui, "Constraints");

    let mut clearance = opt.config.clearance;
    if ui
        .add(
            egui::DragValue::new(&mut clearance)
                .speed(0.005)
                .range(0.0..=5.0)
                .suffix(" m")
                .prefix("gap "),
        )
        .on_hover_text("required space between packed bodies (0 packs flush)")
        .changed()
    {
        opt.config.clearance = clearance;
    }

    rotation_control(ui, opt);
    layer_control(ui, opt);
    boundary_control(ui, opt);

    let mut keep_groups = opt.config.keep_groups;
    if ui
        .checkbox(&mut keep_groups, "move groups rigidly")
        .on_hover_text("a selection group packs as one piece, keeping its internal spacing")
        .changed()
    {
        opt.config.keep_groups = keep_groups;
    }
    let mut honor_pinned = opt.config.honor_pinned;
    if ui
        .checkbox(&mut honor_pinned, "static bodies are obstacles")
        .on_hover_text("static bodies never move; everything else packs around them")
        .changed()
    {
        opt.config.honor_pinned = honor_pinned;
    }

    let mut bias = opt.config.gravity_bias;
    ui.horizontal(|ui| {
        ui.label("settle toward")
            .on_hover_text("a direction the arrangement drifts while packing (0,0 = none)");
        let cx = ui
            .add(egui::DragValue::new(&mut bias.x).speed(0.05).prefix("x "))
            .changed();
        let cy = ui
            .add(egui::DragValue::new(&mut bias.y).speed(0.05).prefix("y "))
            .changed();
        if cx || cy {
            opt.config.gravity_bias = bias;
        }
        if ui
            .small_button("↓")
            .on_hover_text("pile downward")
            .clicked()
        {
            opt.config.gravity_bias = Vec2::new(0.0, -1.0);
        }
        if ui.small_button("0").on_hover_text("no bias").clicked() {
            opt.config.gravity_bias = Vec2::ZERO;
        }
    });
}

/// Rotation freedom, including the step count for the stepped mode.
fn rotation_control(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    let current = opt.config.rotation;
    let label = match current {
        RotationMode::Fixed => "Keep angles".to_owned(),
        RotationMode::Quarter => "Quarter turns".to_owned(),
        RotationMode::Steps { steps } => format!("{steps} steps"),
        RotationMode::Free => "Any angle".to_owned(),
    };
    let mut next = current;
    egui::ComboBox::from_id_salt("pack-rotation")
        .selected_text(label)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut next, RotationMode::Fixed, "Keep angles")
                .on_hover_text("orientation carries meaning — do not turn anything");
            ui.selectable_value(&mut next, RotationMode::Quarter, "Quarter turns")
                .on_hover_text("the usual choice for boxy parts");
            let stepped = matches!(next, RotationMode::Steps { .. });
            if ui.selectable_label(stepped, "N steps").clicked() && !stepped {
                next = RotationMode::Steps { steps: 8 };
            }
            ui.selectable_value(&mut next, RotationMode::Free, "Any angle")
                .on_hover_text("largest search space; needs the most iterations");
        });
    if let RotationMode::Steps { steps } = next {
        let mut n = steps;
        if ui
            .add(
                egui::DragValue::new(&mut n)
                    .range(1..=64)
                    .prefix("steps ")
                    .speed(0.2),
            )
            .on_hover_text("allowed orientations around a full turn")
            .changed()
        {
            next = RotationMode::Steps { steps: n };
        }
    }
    if next != current {
        opt.config.rotation = next;
    }
}

/// How the 2.5D depth bands enter the no-overlap rule.
fn layer_control(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    let mut layers = opt.config.layers;
    egui::ComboBox::from_id_salt("pack-layers")
        .selected_text(match layers {
            LayerRule::Respect => "Depth-aware",
            LayerRule::Solid => "Flat",
        })
        .show_ui(ui, |ui| {
            for rule in LayerRule::ALL {
                ui.selectable_value(
                    &mut layers,
                    rule,
                    match rule {
                        LayerRule::Respect => "Depth-aware",
                        LayerRule::Solid => "Flat",
                    },
                )
                .on_hover_text(rule.label());
            }
        });
    if layers != opt.config.layers {
        opt.config.layers = layers;
    }
}

/// The container constraint and its numbers.
fn boundary_control(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    let current = opt.config.boundary;
    let mut next = current;
    egui::ComboBox::from_id_salt("pack-boundary")
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(matches!(next, Boundary::Free), "Free")
                .clicked()
            {
                next = Boundary::Free;
            }
            if ui
                .selectable_label(matches!(next, Boundary::Aspect { .. }), "Target aspect")
                .on_hover_text("soft: bias the result toward a width:height ratio")
                .clicked()
                && !matches!(next, Boundary::Aspect { .. })
            {
                next = Boundary::Aspect { ratio: 1.0 };
            }
            if ui
                .selectable_label(matches!(next, Boundary::Rect { .. }), "Rectangle")
                .on_hover_text("hard: nothing may leave the box")
                .clicked()
                && !matches!(next, Boundary::Rect { .. })
            {
                next = Boundary::Rect {
                    width: 4.0,
                    height: 4.0,
                };
            }
            if ui
                .selectable_label(matches!(next, Boundary::Circle { .. }), "Circle")
                .on_hover_text("hard: nothing may leave the disc")
                .clicked()
                && !matches!(next, Boundary::Circle { .. })
            {
                next = Boundary::Circle { radius: 3.0 };
            }
        });

    match &mut next {
        Boundary::Free => {}
        Boundary::Aspect { ratio } => {
            ui.add(
                egui::DragValue::new(ratio)
                    .speed(0.05)
                    .range(0.05..=20.0)
                    .prefix("w:h "),
            )
            .on_hover_text("1 = square, 2 = twice as wide as tall");
        }
        Boundary::Rect { width, height } => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(width)
                        .speed(0.05)
                        .range(0.01..=1000.0)
                        .prefix("w ")
                        .suffix(" m"),
                );
                ui.add(
                    egui::DragValue::new(height)
                        .speed(0.05)
                        .range(0.01..=1000.0)
                        .prefix("h ")
                        .suffix(" m"),
                );
            });
        }
        Boundary::Circle { radius } => {
            ui.add(
                egui::DragValue::new(radius)
                    .speed(0.05)
                    .range(0.01..=1000.0)
                    .prefix("r ")
                    .suffix(" m"),
            );
        }
    }
    if next != current {
        opt.config.boundary = next;
    }
}

/// Stopping rules and the preview pace.
fn convergence_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    widgets::section_header(ui, "Convergence");
    egui::Grid::new("pack-convergence")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("tolerance")
                .on_hover_text("relative improvement below which the run is called converged");
            let mut tolerance = opt.config.tolerance;
            if ui
                .add(
                    egui::DragValue::new(&mut tolerance)
                        .speed(1e-5)
                        .range(1e-9..=1.0)
                        .custom_formatter(|v, _| format!("{v:.1e}")),
                )
                .changed()
            {
                opt.config.tolerance = tolerance;
            }
            ui.end_row();

            ui.label("patience")
                .on_hover_text("iterations without improvement before stopping");
            let mut patience = opt.config.patience;
            if ui
                .add(
                    egui::DragValue::new(&mut patience)
                        .range(1..=100_000)
                        .speed(1.0),
                )
                .changed()
            {
                opt.config.patience = patience;
            }
            ui.end_row();

            ui.label("max iterations")
                .on_hover_text("hard ceiling per attempt");
            let mut max_iterations = opt.config.max_iterations;
            if ui
                .add(
                    egui::DragValue::new(&mut max_iterations)
                        .range(1..=1_000_000)
                        .speed(10.0),
                )
                .changed()
            {
                opt.config.max_iterations = max_iterations;
            }
            ui.end_row();

            ui.label("iters / frame")
                .on_hover_text("higher finishes sooner; lower makes the ghost readable");
            let mut per_frame = opt.config.iterations_per_frame;
            if ui
                .add(
                    egui::DragValue::new(&mut per_frame)
                        .range(1..=10_000)
                        .speed(1.0),
                )
                .changed()
            {
                opt.config.iterations_per_frame = per_frame;
            }
            ui.end_row();

            ui.label("max step")
                .on_hover_text("furthest a body may move in one iteration");
            let mut max_step = opt.config.max_step;
            if ui
                .add(
                    egui::DragValue::new(&mut max_step)
                        .speed(0.01)
                        .range(1e-4..=100.0)
                        .suffix(" m"),
                )
                .changed()
            {
                opt.config.max_step = max_step;
            }
            ui.end_row();
        });
}

/// Per-solver tuning, shown only for the solver that uses it.
fn tuning_controls(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    widgets::section_header(ui, "Tuning");
    match opt.config.solver {
        SolverKind::Naive => naive_tuning(ui, opt),
        SolverKind::Shelf => shelf_tuning(ui, opt),
        // L-BFGS has no knobs worth exposing: the line search and the
        // curvature memory are argmin's business, and the iteration budget
        // and goal weights above already cover everything a user would want
        // to reach for.
        SolverKind::Descent => {
            widgets::empty_state(
                ui,
                "Gradient descent is tuned by the goal weights and the iteration budget.",
            );
        }
    }
}

/// The baseline's two gains — exposed because the *ratio* between them is
/// the point being demonstrated: it, and nothing about the packing, decides
/// where this method stops.
fn naive_tuning(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    let mut p = opt.config.naive;
    let mut changed = false;
    egui::Grid::new("pack-naive")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            changed |= widgets::labelled_drag(
                ui,
                "separation",
                &mut p.separation_gain,
                0.01..=1.0,
                0.01,
                "fraction of each overlap resolved per iteration",
            );
            changed |= widgets::labelled_drag(
                ui,
                "attraction",
                &mut p.attraction,
                0.0..=0.5,
                0.005,
                "inward pull per iteration — turn it up and bodies interpenetrate, \
                 down and they stop with visible slack. There is no setting that \
                 packs tightly, which is the finding",
            );
        });
    if changed {
        opt.config.naive = p;
    }
}

fn shelf_tuning(ui: &mut egui::Ui, opt: &mut OptimizerPanel) {
    let mut p = opt.config.shelf;
    let mut changed = false;
    egui::ComboBox::from_id_salt("pack-shelf-order")
        .selected_text(p.order.label())
        .show_ui(ui, |ui| {
            for order in ShelfOrder::ALL {
                changed |= ui
                    .selectable_value(&mut p.order, order, order.label())
                    .changed();
            }
        });
    changed |= ui
        .checkbox(&mut p.descending, "largest first")
        .on_hover_text("off = smallest first")
        .changed();
    changed |= ui
        .checkbox(&mut p.try_rotations, "try turning each part")
        .on_hover_text("test the allowed orientations and keep the tidiest fit")
        .changed();
    if changed {
        opt.config.shelf = p;
    }
}
