//! Reflection-driven property grid: renders any `Reflect` struct's fields
//! as widgets — backend settings get UI for free, no hand-built menus.
//!
//! Supported leaf types: `f32`, `bool`, `u32`, `Vec2`, and any single-field
//! newtype over one of them (a `gradiance-units` quantity such as
//! `Length(f32)` or `Acceleration2(Vec2)` edits as the number it wraps, the
//! way the script bridge reads it). A quantity newtype is recognised by its
//! type name ([`Dimension::of_quantity`]) and shows and parses in the
//! preferred unit (brief 5-3), so a new settings field gets units with no
//! table row. Nested structs recurse; unsupported fields render read-only (the explicit escape
//! hatch is a hand-written row next to the grid — see the grid-system
//! picker in `settings.rs`).

use bevy::math::Vec2;
use bevy::reflect::{Reflect, ReflectMut, ReflectRef};
use bevy_egui::egui::{self, Ui};
use gradiance_units::{Dimension, DisplayUnit};

/// Renders editable widgets for every reflected field of `value`.
/// Returns true if anything changed.
pub fn reflect_grid(ui: &mut Ui, id: egui::Id, value: &mut dyn Reflect) -> bool {
    reflect_grid_units(ui, id, value, &[])
}

/// [`reflect_grid`] with units for plain-`f32` fields: `units` maps a field
/// name to the unit its number is **stored** in (a settings angle kept in
/// degrees is [`DisplayUnit::DEGREES`]). Such a field shows and parses in
/// the preferred unit for that dimension. Quantity newtypes need no row.
pub fn reflect_grid_units(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut dyn Reflect,
    units: &[(&str, DisplayUnit)],
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
    units: &[(&str, DisplayUnit)],
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
            let stored = units.iter().find(|(n, _)| *n == name).map(|(_, u)| *u);
            // A stored-in-degrees name says so (`rotation_step_deg`); the
            // field shows the preferred unit, so the label drops it.
            let pretty = name.trim_end_matches("_deg").replace('_', " ");
            ui.label(pretty);
            changed |= leaf_widget(ui, id.with(name), field, stored);
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

/// One field's widget. `stored` is the unit a number is kept in, when it is
/// a quantity (from the caller's table, or the newtype's own dimension).
fn leaf_widget(
    ui: &mut Ui,
    id: egui::Id,
    field: &mut dyn bevy::reflect::PartialReflect,
    stored: Option<DisplayUnit>,
) -> bool {
    if let (Some(v), Some(unit)) = (field.try_downcast_mut::<f32>(), stored) {
        return crate::unit_fields::stored_drag(ui, v, None, 0.5, unit).changed();
    }
    if let (Some(v), Some(unit)) = (field.try_downcast_mut::<Vec2>(), stored) {
        let mut changed = false;
        ui.horizontal(|ui| {
            for c in [&mut v.x, &mut v.y] {
                changed |= crate::unit_fields::stored_drag(ui, c, None, 0.5, unit).changed();
            }
        });
        return changed;
    }
    if let Some(v) = field.try_downcast_mut::<f32>() {
        return ui
            .add(
                egui::DragValue::new(v)
                    .speed(0.5)
                    // Typed maths (`2*pi`, `90/4`): the one parser.
                    .custom_parser(|t| {
                        DisplayUnit::canonical(Dimension::Dimensionless)
                            .parse(t)
                            .ok()
                            .map(f64::from)
                    }),
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
    // Dimensional newtype (`Length(f32)`, `Acceleration2(Vec2)`): edit the
    // value it wraps, so a typed quantity renders as the number it was.
    let dimension = field
        .get_represented_type_info()
        .and_then(|info| info.type_path_table().ident())
        .and_then(Dimension::of_quantity);
    if let ReflectMut::TupleStruct(ts) = field.reflect_mut()
        && ts.field_len() == 1
        && let Some(inner) = ts.field_mut(0)
    {
        let stored = stored.or(dimension.map(DisplayUnit::canonical));
        return leaf_widget(ui, id, inner, stored);
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

    /// Every text the grid paints for `value` in one headless frame.
    fn painted_text(value: &mut dyn Reflect) -> Vec<String> {
        painted_text_in(value, gradiance_units::UnitPrefs::default())
    }

    /// [`painted_text`] under a unit display preference.
    fn painted_text_in(value: &mut dyn Reflect, prefs: gradiance_units::UnitPrefs) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::unit_fields::install(&ctx, prefs);
        // Two frames: a grid's first frame may be an invisible sizing pass.
        // egui 0.36 debug-asserts that a `TexturesDelta` is applied or cleared
        // before it drops; no renderer exists here, so clear each one.
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                reflect_grid(ui, egui::Id::new("probe"), value);
            });
            output.textures_delta.clear();
            shapes = output.shapes;
        }
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                _ => None,
            })
            .collect()
    }

    #[derive(bevy::reflect::Reflect, Default)]
    struct Probe {
        length: gradiance_units::Length,
        gravity: gradiance_units::Acceleration2,
    }

    #[derive(bevy::reflect::Reflect, Default)]
    struct Degrees {
        rotation_step_deg: f32,
    }

    /// A plain `f32` kept in degrees shows the preferred angle unit, and its
    /// label drops the `_deg` the field name carries.
    #[test]
    fn a_degree_field_follows_the_angle_preference() {
        let mut value = Degrees {
            rotation_step_deg: 90.0,
        };
        let draw = |value: &mut Degrees, angle| {
            let ctx = egui::Context::default();
            crate::unit_fields::install(
                &ctx,
                gradiance_units::UnitPrefs {
                    angle,
                    ..Default::default()
                },
            );
            let mut shapes = Vec::new();
            for _ in 0..2 {
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    reflect_grid_units(
                        ui,
                        egui::Id::new("deg"),
                        value,
                        &[("rotation_step_deg", DisplayUnit::DEGREES)],
                    );
                });
                output.textures_delta.clear();
                shapes = output.shapes;
            }
            shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let text = draw(&mut value, gradiance_units::AngleUnit::Degrees);
        assert!(text.iter().any(|t| t == "rotation step"), "{text:?}");
        assert!(text.iter().any(|t| t == "90\u{b0}"), "{text:?}");
        let text = draw(&mut value, gradiance_units::AngleUnit::Radians);
        assert!(text.iter().any(|t| t.starts_with("1.57") && t.ends_with(" rad")), "{text:?}");
        assert!((value.rotation_step_deg - 90.0).abs() < 1e-6, "showing never writes");
    }

    #[derive(bevy::reflect::Reflect, Default)]
    struct Unsupported {
        name: String,
    }

    #[test]
    fn a_quantity_newtype_renders_as_its_number_with_its_unit() {
        let mut probe = Probe {
            length: gradiance_units::Length(1.25),
            gravity: gradiance_units::Acceleration2(Vec2::new(0.5, -9.75)),
        };
        let text = painted_text(&mut probe);
        assert!(!text.iter().any(|t| t.contains("unsupported")), "{text:?}");
        let accel = gradiance_units::Acceleration::UNIT;
        for shown in ["1.25 m".to_owned(), format!("0.5 {accel}"), format!("-9.75 {accel}")] {
            assert!(text.iter().any(|t| *t == shown), "no {shown} in {text:?}");
        }
        // The display preference reaches the grid: a length in centimetres.
        let cm = gradiance_units::UnitPrefs {
            length: gradiance_units::LengthUnit::Centimetres,
            ..Default::default()
        };
        let text = painted_text_in(&mut probe, cm);
        assert!(text.iter().any(|t| t == "125 cm"), "{text:?}");
        // The detector itself: a genuinely unsupported field says so.
        let text = painted_text(&mut Unsupported::default());
        assert!(text.iter().any(|t| t.contains("unsupported")), "{text:?}");
    }
}
