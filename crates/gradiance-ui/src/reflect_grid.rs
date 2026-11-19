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

use crate::widgets::config_field;
use bevy::math::Vec2;
use bevy::reflect::{PartialReflect, Reflect, ReflectMut, ReflectRef};
use bevy_egui::egui::{self, Ui};
use gradiance_units::{Dimension, DisplayUnit};

/// Renders editable widgets for every reflected field of `value`.
/// Returns true if anything changed. A number resets (Backspace, middle
/// click, Reset) to the same field of `T::default()` (brief U-8b).
pub fn reflect_grid<T: Reflect + Default>(ui: &mut Ui, id: egui::Id, value: &mut T) -> bool {
    reflect_grid_units(ui, id, value, &[])
}

/// [`reflect_grid`] with units for plain-`f32` fields: `units` maps a field
/// name to the unit its number is **stored** in (a settings angle kept in
/// degrees is [`DisplayUnit::DEGREES`]). Such a field shows and parses in
/// the preferred unit for that dimension. Quantity newtypes need no row.
pub fn reflect_grid_units<T: Reflect + Default>(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut T,
    units: &[(&str, DisplayUnit)],
) -> bool {
    reflect_grid_except(ui, id, value, units, &[])
}

/// [`reflect_grid_units`] minus the fields named in `skip` — the ones the
/// caller already drew by hand (an escape-hatch row such as the grid-system
/// picker), so they don't appear a second time as "(unsupported)".
pub fn reflect_grid_except<T: Reflect + Default>(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut T,
    units: &[(&str, DisplayUnit)],
    skip: &[&str],
) -> bool {
    let defaults = T::default();
    grid(ui, id, value, Some(&defaults), units, skip)
}

/// The grid over a reflected struct; `defaults` is a value of the same
/// type whose fields the numbers reset to.
fn grid(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut dyn Reflect,
    defaults: Option<&dyn Reflect>,
    units: &[(&str, DisplayUnit)],
    skip: &[&str],
) -> bool {
    let mut changed = false;
    let ReflectMut::Struct(s) = value.reflect_mut() else {
        return false;
    };
    let defaults = match defaults.map(|d| d.reflect_ref()) {
        Some(ReflectRef::Struct(d)) => Some(d),
        _ => None,
    };
    let names = grid_rows(s, skip);
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for name in &names {
            let Some(field) = s.field_mut(name) else {
                continue;
            };
            let default = defaults.and_then(|d| d.field(name));
            let stored = units.iter().find(|(n, _)| *n == name).map(|(_, u)| *u);
            // A stored-in-degrees name says so (`rotation_step_deg`); the
            // field shows the preferred unit, so the label drops it.
            let pretty = match stored {
                Some(_) => name.trim_end_matches("_deg"),
                None => name,
            }
            .replace('_', " ");
            ui.label(pretty);
            changed |= leaf_widget(ui, id.with(name), field, default, stored);
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

/// One field's widget. `default` is the same field of the type's default
/// value; `stored` is the unit a number is kept in, when it is a quantity
/// (from the caller's table, or the newtype's own dimension).
fn leaf_widget(
    ui: &mut Ui,
    id: egui::Id,
    field: &mut dyn PartialReflect,
    default: Option<&dyn PartialReflect>,
    stored: Option<DisplayUnit>,
) -> bool {
    // A plain number is a dimensionless field: the same behaviours (typed
    // maths, Ctrl snap, Esc, reset; briefs U-8, U-8b), no unit.
    let unit = stored.unwrap_or(DisplayUnit::canonical(Dimension::Dimensionless));
    let number = |ui: &mut Ui, value: &mut f32, default: Option<f32>| {
        let field = config_field(default.unwrap_or(0.0), 0.5).stored(unit);
        let field = if default.is_some() {
            field
        } else {
            field.no_default()
        };
        field.show(ui, value).changed()
    };
    if let Some(v) = field.try_downcast_mut::<f32>() {
        let default = default.and_then(|d| d.try_downcast_ref::<f32>()).copied();
        return number(ui, v, default);
    }
    if let Some(v) = field.try_downcast_mut::<Vec2>() {
        let default = default.and_then(|d| d.try_downcast_ref::<Vec2>()).copied();
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= number(ui, &mut v.x, default.map(|d| d.x));
            changed |= number(ui, &mut v.y, default.map(|d| d.y));
        });
        return changed;
    }
    if let Some(v) = field.try_downcast_mut::<bool>() {
        return ui.checkbox(v, "").changed();
    }
    if let Some(v) = field.try_downcast_mut::<u32>() {
        let default = default.and_then(|d| d.try_downcast_ref::<u32>()).copied();
        let field = config_field(default.unwrap_or(0) as f32, 0.1).range(0.0..=u32::MAX as f32);
        let field = if default.is_some() {
            field
        } else {
            field.no_default()
        };
        return field.show_int(ui, v).changed();
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
        let default = match default.map(|d| d.reflect_ref()) {
            Some(ReflectRef::TupleStruct(d)) => d.field(0),
            _ => None,
        };
        return leaf_widget(ui, id, inner, default, stored);
    }
    // Nested struct: recurse in an indented block.
    if matches!(field.reflect_ref(), ReflectRef::Struct(_)) {
        let mut changed = false;
        ui.vertical(|ui| {
            if let Some(reflect) = field.try_as_reflect_mut() {
                let default = default.and_then(|d| d.try_as_reflect());
                changed = grid(ui, id.with("nested"), reflect, default, &[], &[]);
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
    fn painted_text<T: Reflect + Default>(value: &mut T) -> Vec<String> {
        painted_text_in(value, gradiance_units::UnitPrefs::default())
    }

    /// [`painted_text`] under a unit display preference.
    fn painted_text_in<T: Reflect + Default>(
        value: &mut T,
        prefs: gradiance_units::UnitPrefs,
    ) -> Vec<String> {
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
        assert!(text.concat().contains("90.0\u{b0}"), "{text:?}");
        let text = draw(&mut value, gradiance_units::AngleUnit::Radians);
        let joined = text.concat();
        assert!(
            joined.contains("1.57") && joined.contains(" rad"),
            "{text:?}"
        );
        assert!(
            (value.rotation_step_deg - 90.0).abs() < 1e-6,
            "showing never writes"
        );
    }

    /// A settings struct whose defaults are not zero.
    #[derive(bevy::reflect::Reflect, Debug, PartialEq)]
    struct Tuned {
        gain: f32,
        count: u32,
        reach: gradiance_units::Length,
    }

    impl Default for Tuned {
        fn default() -> Self {
            Self {
                gain: 1.5,
                count: 3,
                reach: gradiance_units::Length(2.0),
            }
        }
    }

    /// Brief U-8b: a grid number resets to the same field of the type's
    /// default on a middle click (a float, a count, a quantity newtype).
    #[test]
    fn a_middle_click_resets_a_grid_number_to_the_type_default() {
        use egui::accesskit::Role;
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut h = Harness::new_ui_state(
            |ui, t: &mut Tuned| {
                reflect_grid(ui, egui::Id::new("tuned"), t);
            },
            Tuned {
                gain: 4.0,
                count: 9,
                reach: gradiance_units::Length(7.0),
            },
        );
        h.run();
        let centres: Vec<egui::Pos2> = h
            .get_all_by_role(Role::SpinButton)
            .map(|n| n.rect().center())
            .collect();
        assert_eq!(centres.len(), 3);
        for at in centres {
            h.hover_at(at);
            h.run();
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Middle,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                h.run();
            }
        }
        assert_eq!(*h.state(), Tuned::default());
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
        for shown in [
            "1.25 m".to_owned(),
            format!("0.5 {accel}"),
            format!("-9.75 {accel}"),
        ] {
            // egui paints the number and the suffix as two texts.
            assert!(text.concat().contains(&shown), "no {shown} in {text:?}");
        }
        // The display preference reaches the grid: a length in centimetres.
        let cm = gradiance_units::UnitPrefs {
            length: gradiance_units::LengthUnit::Centimetres,
            ..Default::default()
        };
        let text = painted_text_in(&mut probe, cm);
        assert!(text.concat().contains("125 cm"), "{text:?}");
        // The detector itself: a genuinely unsupported field says so.
        let text = painted_text(&mut Unsupported::default());
        assert!(text.iter().any(|t| t.contains("unsupported")), "{text:?}");
    }
}
