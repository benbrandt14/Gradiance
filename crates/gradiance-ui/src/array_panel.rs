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
use gradiance_interaction::tools::array_tool::{ArrayConfig, ArraySpacing, MAX_COPIES_PER_AXIS};

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
    bond_controls(ui, config);
    ui.separator();
    spacing_controls(ui, config);
    ui.separator();
    count_controls(ui, config);
    ui.separator();
    tween_controls(ui, config);
}

/// Pattern family and its own parameters.
fn bond_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Bond").strong());
    let mut stagger = config.stagger;
    if ui
        .add(
            egui::DragValue::new(&mut stagger)
                .speed(0.01)
                .range(0.0..=1.0)
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
                    .range(0.0..=100.0)
                    .suffix(" m")
                    .prefix("gap "),
            )
            .on_hover_text(
                "added to the flush pitch. It cannot go negative: a pattern \
                 that overlaps itself is never what was wanted",
            );
        }
    }
}

/// Whether the drag decides the count, or the user does.
fn count_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Count").strong());
    ui.label(
        egui::RichText::new(
            "fix a count and the drag sets the spacing instead — pull to \
             spread that many copies over the distance",
        )
        .weak()
        .small(),
    );
    axis_count(ui, "columns (X)", &mut config.count_x);
    axis_count(ui, "rows (Y)", &mut config.count_y);
}

/// One axis' fixed-count control: a checkbox that turns the drag from
/// "how many" into "how far apart", plus the count itself.
fn axis_count(ui: &mut egui::Ui, label: &str, slot: &mut Option<u32>) {
    ui.horizontal(|ui| {
        let mut fixed = slot.is_some();
        if ui.checkbox(&mut fixed, label).changed() {
            *slot = fixed.then_some(slot.unwrap_or(4));
        }
        if let Some(count) = slot {
            ui.add(
                egui::DragValue::new(count)
                    .speed(0.2)
                    .range(1..=MAX_COPIES_PER_AXIS),
            );
        }
    });
}

/// Per-copy changes along the pattern.
fn tween_controls(ui: &mut egui::Ui, config: &mut ArrayConfig) {
    ui.label(egui::RichText::new("Per copy").strong());
    ui.label(
        egui::RichText::new(
            "what changes from one copy to the next — one lane per pattern \
             axis, each size axis on its own",
        )
        .weak()
        .small(),
    );
    let mut t = config.tweens;
    let mut changed = false;
    // Two lanes, because a grid moves two ways: the X lane fires once per
    // column, the Y lane once per row. A row or a column only ever drives the
    // lane named after the direction it runs.
    changed |= lane_controls(ui, "along X (columns)", "array-tween-x", &mut t.along_x);
    ui.add_space(4.0);
    changed |= lane_controls(ui, "along Y (rows)", "array-tween-y", &mut t.along_y);

    if changed {
        config.tweens = t;
    }
    ui.add_space(2.0);
    if !config.tweens.is_identity() {
        ui.label(
            egui::RichText::new(
                "contact spacing follows the size taper: copies close up as \
                 they shrink",
            )
            .weak()
            .small(),
        );
    }
    ui.horizontal(|ui| {
        if ui.small_button("reset").clicked() {
            config.tweens = gradiance_command::array_cmd::ArrayTweens::default();
        }
        if ui
            .small_button("shrink 1%")
            .on_hover_text("the classic taper: every copy 99% of the last, both axes")
            .clicked()
        {
            config.tweens.along_x.scale = Vec2::splat(0.99);
        }
    });
}

/// One lane of per-copy change: every field for a single pattern axis.
///
/// Returns whether anything was edited. Sizes get one control per axis rather
/// than a single ratio, so "narrow as it goes" and "flatten as it goes" are
/// separate, sayable things.
fn lane_controls(
    ui: &mut egui::Ui,
    title: &str,
    salt: &str,
    lane: &mut gradiance_command::array_cmd::TweenStep,
) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new(title).small().strong());
    egui::Grid::new(salt)
        .num_columns(2)
        .spacing([8.0, 2.0])
        .show(ui, |ui| {
            ui.label("turn").on_hover_text(
                "extra rotation per copy — a fan of blades, or a spiral when \
                 combined with a radial sweep",
            );
            let mut spin_deg = lane.spin.to_degrees();
            if ui
                .add(
                    egui::DragValue::new(&mut spin_deg)
                        .speed(0.5)
                        .range(-180.0..=180.0)
                        .suffix("°"),
                )
                .changed()
            {
                lane.spin = spin_deg.to_radians();
                changed = true;
            }
            ui.end_row();

            ui.label("scale x")
                .on_hover_text("width ratio per copy — 0.99 narrows each one by a percent");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut lane.scale.x)
                        .speed(0.005)
                        .range(0.05..=20.0),
                )
                .changed();
            ui.end_row();

            ui.label("scale y")
                .on_hover_text("height ratio per copy — set it apart from x to taper a grid");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut lane.scale.y)
                        .speed(0.005)
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
                    egui::DragValue::new(&mut lane.depth)
                        .speed(0.005)
                        .range(-10.0..=10.0)
                        .suffix(" m"),
                )
                .changed();
            ui.end_row();
        });
    changed
}
