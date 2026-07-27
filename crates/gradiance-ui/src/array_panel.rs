//! The **Array** window: the rulebook for alt-dragging a scale handle into a
//! repeated pattern.
//!
//! The gesture itself needs no UI — grab a handle, hold `Alt`, pull, and the
//! copies appear at flush contact. This window is for everything the drag
//! cannot say: what *kind* of repetition, how the spacing is derived, and
//! what should change from one copy to the next.
//!
//! Like the Optimizer window it edits a settings resource directly (the
//! sanctioned Config seam) and never touches the world; the array still lands
//! through the ordinary intent path as one undoable command.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_interaction::tools::array_tool::{
    ArrayConfig, ArrayPattern, ArraySpacing, MAX_COPIES_PER_AXIS,
};

/// Whether the floating Array window is showing.
#[derive(Resource, Default, Debug)]
pub struct ArrayWindow {
    /// Window visibility.
    pub open: bool,
}

impl ArrayWindow {
    /// Flips the window open/closed.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }
}

/// Renders the floating Array window.
pub fn array_window(
    mut contexts: EguiContexts,
    mut window: ResMut<ArrayWindow>,
    mut config: ResMut<ArrayConfig>,
) -> Result {
    if !window.open {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let mut open = true;
    egui::Window::new("Array")
        .open(&mut open)
        .default_width(320.0)
        .default_height(460.0)
        .resizable(true)
        .vscroll(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| array_body(ui, &mut config));
        });
    if !open {
        window.open = false;
    }
    Ok(())
}

/// The window contents.
fn array_body(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(
        egui::RichText::new(
            "Hold Alt and drag a selection handle. Side handles repeat along \
             one axis, corners make a grid.",
        )
        .weak()
        .small(),
    );
    ui.separator();
    pattern_controls(ui, config);
    ui.separator();
    spacing_controls(ui, config);
    ui.separator();
    count_controls(ui, config);
    ui.separator();
    tween_controls(ui, config);
}

/// Pattern family and its own parameters.
fn pattern_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Pattern").strong());
    let mut pattern = config.pattern;
    egui::ComboBox::from_id_salt("array-pattern")
        .selected_text(pattern.label())
        .show_ui(ui, |ui| {
            for kind in ArrayPattern::ALL {
                ui.selectable_value(&mut pattern, kind, kind.label());
            }
        });
    if pattern != config.pattern {
        config.pattern = pattern;
    }

    match config.pattern {
        ArrayPattern::Repeat => {
            let mut stagger = config.stagger;
            if ui
                .add(
                    egui::DragValue::new(&mut stagger)
                        .speed(0.01)
                        .range(-1.0..=1.0)
                        .prefix("row offset "),
                )
                .on_hover_text(
                    "fraction of a step that alternate grid rows shift by — \
                     0.5 gives a running-bond brick wall",
                )
                .changed()
            {
                config.stagger = stagger;
            }
            ui.horizontal(|ui| {
                if ui.small_button("stack bond").clicked() {
                    config.stagger = 0.0;
                }
                if ui.small_button("brick bond").clicked() {
                    config.stagger = 0.5;
                }
            });
        }
        ArrayPattern::Radial => {
            let mut degrees = config.angle_step.to_degrees();
            if ui
                .add(
                    egui::DragValue::new(&mut degrees)
                        .speed(0.5)
                        .range(-360.0..=360.0)
                        .suffix("°")
                        .prefix("step "),
                )
                .on_hover_text("angle between consecutive copies")
                .changed()
            {
                config.angle_step = degrees.to_radians();
            }
            ui.horizontal(|ui| {
                // The counts that close a full circle exactly.
                for n in [4u32, 6, 8, 12] {
                    if ui.small_button(format!("{n}/turn")).clicked() {
                        config.angle_step = std::f32::consts::TAU / n as f32;
                    }
                }
            });
            let mut rotate = config.rotate_items;
            if ui
                .checkbox(&mut rotate, "turn bodies with the sweep")
                .on_hover_text("off = bodies orbit but stay upright")
                .changed()
            {
                config.rotate_items = rotate;
            }
        }
    }
}

/// How the step is derived from the selection's geometry.
fn spacing_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Spacing").strong());
    let current = config.spacing;
    let mut next = current;
    egui::ComboBox::from_id_salt("array-spacing")
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for option in ArraySpacing::ALL {
                // Compare discriminants, not values: switching to "Contact +
                // gap" should keep whatever gap is already dialled in.
                let selected = std::mem::discriminant(&next) == std::mem::discriminant(&option);
                if ui.selectable_label(selected, option.label()).clicked() && !selected {
                    next = option;
                }
            }
        });
    if next != current {
        config.spacing = next;
    }

    match &mut config.spacing {
        ArraySpacing::Contact => {
            ui.label(
                egui::RichText::new(
                    "Copies land flush — the exact smallest step that clears \
                     the selection from itself along the drag.",
                )
                .weak()
                .small(),
            );
        }
        ArraySpacing::Gap(gap) => {
            ui.add(
                egui::DragValue::new(gap)
                    .speed(0.005)
                    .range(-5.0..=100.0)
                    .suffix(" m")
                    .prefix("gap "),
            )
            .on_hover_text("added to the flush pitch; negative deliberately overlaps");
        }
        ArraySpacing::Fixed(step) => {
            ui.add(
                egui::DragValue::new(step)
                    .speed(0.01)
                    .range(0.001..=1000.0)
                    .suffix(" m")
                    .prefix("step "),
            )
            .on_hover_text("ignores the geometry entirely");
        }
        ArraySpacing::Multiple(factor) => {
            ui.add(
                egui::DragValue::new(factor)
                    .speed(0.05)
                    .range(0.05..=100.0)
                    .prefix("× contact "),
            )
            .on_hover_text("2.0 leaves a body-sized hole; 0.5 interleaves copies");
        }
    }
}

/// Whether the drag decides the count, or the user does.
fn count_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Count").strong());
    let mut fixed = config.count_override.is_some();
    if ui
        .checkbox(&mut fixed, "fixed count")
        .on_hover_text("off = the drag distance decides how many copies fit")
        .changed()
    {
        config.count_override = fixed.then_some(config.count_override.unwrap_or(4));
    }
    if let Some(count) = &mut config.count_override {
        ui.add(
            egui::DragValue::new(count)
                .speed(0.2)
                .range(1..=MAX_COPIES_PER_AXIS)
                .prefix("copies "),
        );
    }
}

/// Per-copy changes along the pattern.
fn tween_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Per copy").strong());
    ui.label(
        egui::RichText::new("changes that accumulate along the pattern")
            .weak()
            .small(),
    );
    let mut t = config.tweens;
    let mut changed = false;
    egui::Grid::new("array-tweens")
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("turn").on_hover_text(
                "extra rotation per copy — a fan of blades, or a spiral when \
                 combined with a radial sweep",
            );
            let mut spin_deg = t.spin.to_degrees();
            if ui
                .add(
                    egui::DragValue::new(&mut spin_deg)
                        .speed(0.5)
                        .range(-180.0..=180.0)
                        .suffix("°"),
                )
                .changed()
            {
                t.spin = spin_deg.to_radians();
                changed = true;
            }
            ui.end_row();

            ui.label("taper")
                .on_hover_text("size ratio per copy — 0.9 shrinks each one, 1.1 grows it");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut t.scale_ratio)
                        .speed(0.01)
                        .range(0.05..=20.0),
                )
                .changed();
            ui.end_row();

            ui.label("depth step").on_hover_text(
                "shifts each copy further into the screen — a staircase through \
                 the collision layers, so copies past one layer stop colliding",
            );
            changed |= ui
                .add(
                    egui::DragValue::new(&mut t.depth)
                        .speed(0.005)
                        .range(-10.0..=10.0)
                        .suffix(" m"),
                )
                .changed();
            ui.end_row();
        });
    if changed {
        config.tweens = t;
    }
    if ui.small_button("reset").clicked() {
        config.tweens = gradiance_command::array_cmd::ArrayTweens::default();
    }
}
