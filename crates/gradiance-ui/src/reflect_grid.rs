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
    reflect_grid_units(ui, id, value, &[])
}

/// [`reflect_grid`] with per-field SI unit labels — `units` maps a field name
/// to its unit symbol (from the `Dimension` catalog, so labels can't drift).
/// Fields not in the map render label-only, so partial coverage is fine.
pub fn reflect_grid_units(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut dyn Reflect,
    units: &[(&str, &str)],
) -> bool {
    reflect_grid_except(ui, id, value, units, &[])
}

/// [`reflect_grid_units`] minus the fields named in `skip` — the ones the
/// caller already drew by hand (an escape-hatch row such as the grid-system
/// picker), so they don't appear a second time as "(unsupported)".
pub fn reflect_grid_except(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut dyn Reflect,
    units: &[(&str, &str)],
    skip: &[&str],
) -> bool {
    let mut changed = false;
    let ReflectMut::Struct(s) = value.reflect_mut() else {
        return false;
    };
    let names = grid_rows(s, skip);
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for name in &names {
            let Some(field) = s.field_mut(name) else {
                continue;
            };
            let unit = units
                .iter()
                .find(|(n, _)| *n == name)
                .map_or("", |(_, u)| *u);
            let pretty = name.replace('_', " ");
            if unit.is_empty() {
                ui.label(pretty);
            } else {
                ui.label(format!("{pretty}  ({unit})"));
            }
            changed |= leaf_widget(ui, id.with(name), field);
            ui.end_row();
        }
    });
    changed
}

/// The field names the grid draws a row for: every field of `s` in
/// declaration order, minus `skip`.
fn grid_rows(s: &dyn bevy::reflect::structs::Struct, skip: &[&str]) -> Vec<String> {
    (0..s.field_len())
        .filter_map(|i| s.name_at(i))
        .filter(|name| !skip.contains(name))
        .map(str::to_owned)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::GRID_HAND_DRAWN;
    use bevy::reflect::PartialReflect;
    use gradiance_domain::settings::GridSettings;

    #[test]
    fn grid_snap_rows_skip_the_hand_drawn_system_picker() {
        let grid = GridSettings::default();
        let ReflectRef::Struct(s) = grid.reflect_ref() else {
            panic!("GridSettings reflects as a struct");
        };
        assert!(grid_rows(s, &[]).iter().any(|n| n == "system"));
        let rows = grid_rows(s, GRID_HAND_DRAWN);
        assert!(!rows.iter().any(|n| n == "system"), "rows: {rows:?}");
        assert!(rows.iter().any(|n| n == "spacing"), "rows: {rows:?}");
    }
}
