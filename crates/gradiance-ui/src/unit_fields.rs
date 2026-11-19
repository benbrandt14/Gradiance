//! Unit-aware number fields (brief 5-3): the display preference and the one
//! drag that every quantity field goes through.
//!
//! [`publish_unit_prefs`] copies the workstation preference
//! ([`UnitSettings`]) into the egui context once per frame, so a field looks
//! its unit up from the `Ui` it is drawn in: no panel threads the resource
//! through. [`drag_value`] shows a base-SI value in a [`DisplayUnit`] and
//! parses a typed entry back through [`DisplayUnit::parse`] (the one parser,
//! [`gradiance_units::expr`]). [`crate::widgets::precise_drag_unit`] (edit
//! seam), [`crate::widgets::labelled_quantity`] (config seam) and the
//! reflect grid's quantity rows all call it.

use bevy::prelude::*;
use bevy_egui::EguiContexts;
use bevy_egui::egui::{self, Ui};
use gradiance_domain::settings::UnitSettings;
use gradiance_units::{Dimension, DisplayUnit, UnitPrefs};
use std::ops::RangeInclusive;

/// Where [`install`] keeps the preference in the context's memory.
fn key() -> egui::Id {
    egui::Id::new("gradiance-unit-prefs")
}

/// Makes `prefs` the preference the context's fields read.
pub fn install(ctx: &egui::Context, prefs: UnitPrefs) {
    ctx.data_mut(|d| d.insert_temp(key(), prefs));
}

/// The preference the fields of `ctx` show (the default before the first
/// [`install`], as in a headless test).
#[must_use]
pub fn prefs(ctx: &egui::Context) -> UnitPrefs {
    ctx.data(|d| d.get_temp(key())).unwrap_or_default()
}

/// The unit a field of dimension `dim` shows in `ctx`.
#[must_use]
pub fn display(ctx: &egui::Context, dim: Dimension) -> DisplayUnit {
    prefs(ctx).display(dim)
}

/// Copies the workstation preference into the primary context, before the
/// panels draw (it runs after `theme::apply_chrome` in the UI pass).
pub fn publish_unit_prefs(
    mut contexts: EguiContexts,
    settings: Option<Res<UnitSettings>>,
) -> Result {
    let prefs = settings.map_or_default(|s| s.prefs);
    install(contexts.ctx_mut()?, prefs);
    Ok(())
}

/// A drag over a base-SI `value` shown in `unit`: `speed` and `range` are
/// base SI too. A typed entry parses through [`DisplayUnit::parse`]; one that
/// fails leaves the value as it was. `value` is written only when the field
/// changed, so showing a value never rounds it.
pub fn drag_value(
    ui: &mut Ui,
    value: &mut f32,
    speed: f32,
    range: Option<RangeInclusive<f32>>,
    unit: DisplayUnit,
) -> egui::Response {
    let shown_of = |si: f32| unit.from_si(f64::from(si));
    let mut shown = shown_of(*value);
    let mut drag = egui::DragValue::new(&mut shown)
        .speed(shown_of(speed))
        .suffix(unit.suffix())
        .custom_parser(move |text| unit.parse(text).ok().map(shown_of));
    if let Some(range) = range {
        drag = drag.range(shown_of(*range.start())..=shown_of(*range.end()));
    }
    let response = ui.add(drag);
    if response.changed() {
        *value = unit.to_si(shown) as f32;
    }
    response
}

/// A drag over a value kept in `stored` (a settings field in degrees, or a
/// canonical SI field), shown in the unit the preference picks for its
/// dimension. `range` and `speed` are in `stored`.
pub fn stored_drag(
    ui: &mut Ui,
    value: &mut f32,
    range: Option<RangeInclusive<f32>>,
    speed: f32,
    stored: DisplayUnit,
) -> egui::Response {
    let to_si = |v: f32| stored.to_si(f64::from(v)) as f32;
    let shown = display(ui.ctx(), stored.dimension);
    let mut si = to_si(*value);
    let range = range.map(|r| to_si(*r.start())..=to_si(*r.end()));
    let response = drag_value(ui, &mut si, to_si(speed), range, shown);
    if response.changed() {
        *value = stored.from_si(f64::from(si)) as f32;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{Commit, precise_drag_unit};
    use gradiance_units::{AngleUnit, LengthUnit};

    /// Every symbol a field can show is drawable: its non-ASCII characters
    /// are listed glyphs (`docs/ui-design.md` § 1).
    #[test]
    fn every_shown_unit_uses_listed_glyphs() {
        let mut units = vec![DisplayUnit::RPM, DisplayUnit::DEGREES];
        for angle in AngleUnit::ALL {
            for length in LengthUnit::ALL {
                let prefs = UnitPrefs { angle, length };
                units.extend(Dimension::ALL.map(|d| prefs.display(d)));
            }
        }
        for unit in units {
            for c in unit.symbol.chars().filter(|c| !c.is_ascii()) {
                let s = c.to_string();
                assert!(
                    crate::fonts::glyph::ALL.contains(&s.as_str()),
                    "`{}` shows {c:?}, which is not in fonts::glyph",
                    unit.symbol
                );
            }
        }
    }

    /// One frame of a field: where it was laid out, its painted text, and
    /// what it committed.
    struct Frame {
        rect: egui::Rect,
        text: Vec<String>,
        commit: Option<(f32, f32)>,
    }

    /// Draws one `precise_drag_unit` length field over `value` for a frame
    /// of `events`.
    fn frame(
        ctx: &egui::Context,
        prefs: UnitPrefs,
        value: &mut f32,
        events: Vec<egui::Event>,
    ) -> Frame {
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        let mut rect = egui::Rect::NOTHING;
        let mut commit = None;
        let mut output = ctx.run_ui(input, |ui| {
            install(ui.ctx(), prefs);
            if let Commit::Done(old, new) = precise_drag_unit(
                ui,
                egui::Id::new("probe"),
                value,
                1.0,
                0.01,
                Dimension::Length,
            ) {
                commit = Some((old, new));
            }
            // The field is the only widget: the Ui's extent is the field.
            rect = ui.min_rect();
        });
        output.textures_delta.clear();
        let text = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_string()),
                _ => None,
            })
            .collect();
        Frame { rect, text, commit }
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        vec![egui::Event::PointerMoved(pos), button(true), button(false)]
    }

    fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Clicks the field, replaces its text with `typed`, presses Enter, and
    /// returns every commit the gesture made plus the text painted after it.
    fn type_into(prefs: UnitPrefs, value: &mut f32, typed: &str) -> (Vec<(f32, f32)>, Vec<String>) {
        let ctx = egui::Context::default();
        let mut commits = Vec::new();
        let first = frame(&ctx, prefs, value, vec![]);
        let at = first.rect.left_center() + egui::vec2(8.0, 0.0);
        let mut script: Vec<Vec<egui::Event>> = vec![
            click_at(at),
            vec![],
            vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
            vec![egui::Event::Text(typed.to_owned())],
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        ];
        script.extend(std::iter::repeat_with(Vec::new).take(3));
        let mut last = Vec::new();
        for events in script {
            let f = frame(&ctx, prefs, value, events);
            commits.extend(f.commit);
            last = f.text;
        }
        (commits, last)
    }

    /// Brief 5-3's done-when: a value typed with its unit commits the right
    /// SI value, once (the edit seam turns one commit into one undo step).
    #[test]
    fn a_typed_unit_value_commits_its_si_value_once() {
        let mut value = 1.0;
        let (commits, text) = type_into(UnitPrefs::default(), &mut value, "25 cm");
        assert_eq!(commits.len(), 1, "one commit: {commits:?}");
        let (old, new) = commits[0];
        assert!((old - 1.0).abs() < 1e-6, "{commits:?}");
        assert!((new - 0.25).abs() < 1e-6, "{commits:?}");
        assert!((value - 0.25).abs() < 1e-6);
        // egui paints the number and the suffix as two texts.
        assert!(text.concat().contains("0.250 m"), "{text:?}");
    }

    /// Under a centimetre preference the same field shows centimetres, a
    /// bare number is centimetres, and the stored value is still metres.
    #[test]
    fn a_bare_number_is_in_the_preferred_unit() {
        let prefs = UnitPrefs {
            length: LengthUnit::Centimetres,
            ..UnitPrefs::default()
        };
        let mut value = 1.0;
        let ctx = egui::Context::default();
        let shown = frame(&ctx, prefs, &mut value, vec![]).text;
        assert!(shown.concat().contains("100.0 cm"), "{shown:?}");
        let (commits, _) = type_into(prefs, &mut value, "40");
        assert_eq!(commits.len(), 1, "{commits:?}");
        assert!((value - 0.4).abs() < 1e-6, "{value}");
    }

    /// An entry that does not parse, or is of another dimension, changes
    /// nothing and commits nothing.
    #[test]
    fn a_wrong_entry_keeps_the_value() {
        for typed in ["90 deg", "3 furlongs", "2 * * 3"] {
            let mut value = 1.0;
            let (commits, _) = type_into(UnitPrefs::default(), &mut value, typed);
            assert!(commits.is_empty(), "{typed}: {commits:?}");
            assert!((value - 1.0).abs() < 1e-6, "{typed}: {value}");
        }
    }
}
