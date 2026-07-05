//! Reflection-driven property grid: renders any `Reflect` struct's fields
//! as widgets — backend settings get UI for free, no hand-built menus.
//!
//! Supported leaf types: `f32`, `bool`, `u32`, `Vec2`. Nested structs
//! recurse; unsupported fields render read-only (the explicit escape
//! hatch is a hand-written row next to the grid — see the grid-system
//! picker in `settings.rs`).

use bevy::math::Vec2;
use bevy::reflect::{Reflect, ReflectMut, ReflectRef};
use bevy_egui::egui::{self, Ui};

/// Renders editable widgets for every reflected field of `value`.
/// Returns true if anything changed.
pub fn reflect_grid(ui: &mut Ui, id: egui::Id, value: &mut dyn Reflect) -> bool {
    let mut changed = false;
    let ReflectMut::Struct(s) = value.reflect_mut() else {
        return false;
    };
    let names: Vec<String> = (0..s.field_len())
        .filter_map(|i| s.name_at(i).map(str::to_owned))
        .collect();
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for name in &names {
            let Some(field) = s.field_mut(name) else {
                continue;
            };
            ui.label(name.replace('_', " "));
            changed |= leaf_widget(ui, id.with(name), field);
            ui.end_row();
        }
    });
    changed
}

fn leaf_widget(ui: &mut Ui, id: egui::Id, field: &mut dyn bevy::reflect::PartialReflect) -> bool {
    if let Some(v) = field.try_downcast_mut::<f32>() {
        return ui
            .add(
                egui::DragValue::new(v)
                    .speed(0.5)
                    .custom_parser(|t| t.trim().parse::<f64>().ok()),
            )
            .changed();
    }
    if let Some(v) = field.try_downcast_mut::<bool>() {
        return ui.checkbox(v, "").changed();
    }
    if let Some(v) = field.try_downcast_mut::<u32>() {
        return ui.add(egui::DragValue::new(v).speed(0.1)).changed();
    }
    if let Some(v) = field.try_downcast_mut::<Vec2>() {
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= ui.add(egui::DragValue::new(&mut v.x).speed(0.5)).changed();
            changed |= ui.add(egui::DragValue::new(&mut v.y).speed(0.5)).changed();
        });
        return changed;
    }
    // Nested struct: recurse in an indented block.
    if matches!(field.reflect_ref(), ReflectRef::Struct(_)) {
        let mut changed = false;
        ui.vertical(|ui| {
            if let Some(reflect) = field.try_as_reflect_mut() {
                changed = reflect_grid(ui, id.with("nested"), reflect);
            }
        });
        return changed;
    }
    ui.label(egui::RichText::new("(unsupported)").weak());
    false
}
