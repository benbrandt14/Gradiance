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
    ui.separator();
    spacing_controls(ui, config);
    ui.separator();
    count_controls(ui, config);
    ui.separator();
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
