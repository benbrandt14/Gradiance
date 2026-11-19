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
use gradiance_units::{Dimension, DisplayUnit, UnitPrefs, step};
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
/// base SI too. The plain form of [`number_field`] (no default, so no
/// Backspace or Reset); `value` is written only when the field changed, so
/// showing a value never rounds it.
pub fn drag_value(
    ui: &mut Ui,
    value: &mut f32,
    speed: f32,
    range: Option<RangeInclusive<f32>>,
    unit: DisplayUnit,
) -> egui::Response {
    number_field(ui, value, speed, range, unit, None).response
}

/// What one frame of a [`number_field`] did.
pub struct Field {
    /// The field's response (`changed` when the value moved this frame).
    pub response: egui::Response,
    /// An edit made in one step this frame (Ctrl+wheel, Backspace, `-`, a
    /// paste, the menu's Reset, a middle-click): the value before it. The
    /// edit seam commits it at once, as one undo step.
    pub instant: Option<f32>,
    /// Esc cancelled the drag this frame: the value is back where the drag
    /// started, and the gesture must commit nothing.
    pub cancelled: bool,
}

/// How long a refused entry's message stays under its field, in seconds.
pub const BAD_ENTRY_SECS: f64 = 4.0;

/// A typed entry the parser refused, kept so the field can say why.
#[derive(Clone)]
struct BadEntry {
    text: String,
    message: String,
    at: f64,
}

/// Where a field records that it wants this frame's keys.
fn claim_key() -> egui::Id {
    egui::Id::new("gradiance-field-claims-keys")
}

/// Where Copy keeps the last copied field text (the menu's Paste reads it:
/// egui gives a widget no way to read the system clipboard).
fn clipboard_key() -> egui::Id {
    egui::Id::new("gradiance-field-clipboard")
}

/// A field that is hovered or dragged takes the keyboard: Backspace, `-`,
/// Esc, Ctrl+C and Ctrl+V are its, not the scene's (delete, deselect,
/// copy and paste bodies). Folded into `KeyboardCaptured` at the end of the
/// pass ([`take_key_claim`]).
pub fn claim_keys(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(claim_key(), true));
}

/// Whether a field claimed the keyboard this pass; clears the claim.
pub fn take_key_claim(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(claim_key()))
        .unwrap_or(false)
}

/// The last text a field's Copy put on the clipboard, if any.
#[must_use]
pub fn copied(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp::<String>(clipboard_key()))
}

/// Keys and wheel turns a hovered field acts on, read in one pass.
#[derive(Default)]
struct HoverInput {
    wheel: i32,
    reset: bool,
    negate: bool,
    copy: bool,
    paste: Option<String>,
}

fn hover_input(ui: &Ui) -> HoverInput {
    ui.input_mut(|i| {
        let mut out = HoverInput::default();
        for event in &i.events {
            match event {
                egui::Event::MouseWheel {
                    delta, modifiers, ..
                } if modifiers.command && delta.y != 0.0 => {
                    out.wheel += if delta.y > 0.0 { 1 } else { -1 };
                }
                egui::Event::Copy => out.copy = true,
                egui::Event::Paste(text) => out.paste = Some(text.clone()),
                _ => {}
            }
        }
        out.reset = i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace);
        out.negate = i.consume_key(egui::Modifiers::NONE, egui::Key::Minus);
        out.copy |= i.consume_key(egui::Modifiers::COMMAND, egui::Key::C);
        // bevy_egui sends Ctrl+V as the clipboard's text (manage_clipboard).
        if i.consume_key(egui::Modifiers::COMMAND, egui::Key::V) && out.paste.is_none() {
            out.paste = i.events.iter().find_map(|e| match e {
                egui::Event::Text(text) => Some(text.clone()),
                _ => None,
            });
        }
        out
    })
}

/// A field's fixed facts for one frame, shared by its behaviours.
struct Spec {
    unit: DisplayUnit,
    range: Option<RangeInclusive<f32>>,
    default: Option<f32>,
    /// The Ctrl-snap step, in the shown unit.
    step: f64,
    bad_id: egui::Id,
}

impl Spec {
    fn shown(&self, si: f32) -> f64 {
        self.unit.from_si(f64::from(si))
    }

    fn si(&self, shown: f64) -> f32 {
        self.unit.to_si(shown) as f32
    }

    fn clamp(&self, si: f32) -> f32 {
        match &self.range {
            Some(r) => si.clamp(*r.start(), *r.end()),
            None => si,
        }
    }

    /// Copies `value` with its unit, to the system clipboard and to the
    /// one the menu's Paste reads.
    fn copy(&self, ctx: &egui::Context, value: f32) {
        let text = self.unit.format(value, 6);
        ctx.copy_text(text.clone());
        ctx.data_mut(|d| d.insert_temp(clipboard_key(), text));
    }
}

/// Parses `text` for a field showing `unit`, in base SI. A refusal is filed
/// under `bad_id` so the field can say why; a success clears it.
fn parse_entry(
    ctx: &egui::Context,
    unit: DisplayUnit,
    bad_id: egui::Id,
    text: &str,
) -> Option<f32> {
    match unit.parse(text) {
        Ok(si) => {
            ctx.data_mut(|d| d.remove::<BadEntry>(bad_id));
            Some(si)
        }
        Err(err) => {
            let bad = BadEntry {
                text: text.trim().to_owned(),
                message: err.to_string(),
                at: ctx.input(|i| i.time),
            };
            ctx.data_mut(|d| d.insert_temp(bad_id, bad));
            None
        }
    }
}

/// The value under one-step edits: remembers the value before the first.
struct Instant<'a> {
    value: &'a mut f32,
    before: Option<f32>,
}

impl Instant<'_> {
    fn set(&mut self, spec: &Spec, new: f32) {
        let new = spec.clamp(new);
        if new.is_finite() {
            self.before.get_or_insert(*self.value);
            *self.value = new;
        }
    }

    fn paste(&mut self, ui: &Ui, spec: &Spec, text: &str) {
        if let Some(si) = parse_entry(ui.ctx(), spec.unit, spec.bad_id, text) {
            self.set(spec, si);
        }
    }
}

/// The number field (brief U-8; `docs/unified-surface-plan.md` § 7.2).
/// `value`, `speed`, `range` and `default` are base SI; the field shows
/// them in `unit`.
///
/// - **Drag** to scrub; **Shift** scrubs at 1/10 speed (egui's own);
///   **Ctrl** snaps to [`gradiance_units::step::snap_step`] (Ctrl+Shift to a
///   tenth of it); **Esc** during a drag puts the value back where the drag
///   started ([`Field::cancelled`]).
/// - **Click** to type: a number or maths with units through
///   [`DisplayUnit::parse`], applied on Enter or focus loss; Esc while
///   typing keeps the old value. A refused entry outlines the field in the
///   theme's error colour and says why for [`BAD_ENTRY_SECS`].
/// - Over a **hovered** field: Backspace = `default`, `-` = negate,
///   Ctrl+wheel = one snap step per notch, Ctrl+C / Ctrl+V copy and paste
///   the value with its unit; middle-click = `default`. These are
///   [`Field::instant`] edits. Right-click: Copy, Paste, Reset.
pub fn number_field(
    ui: &mut Ui,
    value: &mut f32,
    speed: f32,
    range: Option<RangeInclusive<f32>>,
    unit: DisplayUnit,
    default: Option<f32>,
) -> Field {
    // The DragValue's own id (it takes the next auto id), so the parser can
    // file a refusal under it before the response exists.
    let id = ui.next_auto_id();
    let shown_range = range.as_ref().map(|r| {
        (
            unit.from_si(f64::from(*r.start())),
            unit.from_si(f64::from(*r.end())),
        )
    });
    let spec = Spec {
        unit,
        step: step::snap_step(unit, unit.from_si(f64::from(speed)), shown_range),
        range,
        default,
        bad_id: id.with("bad-entry"),
    };
    let before = *value;

    let ctx = ui.ctx().clone();
    let bad_id = spec.bad_id;
    let mut shown = spec.shown(*value);
    let mut drag = egui::DragValue::new(&mut shown)
        .speed(spec.shown(speed))
        .suffix(unit.suffix())
        // Apply a typed entry on Enter or focus loss only: with egui's
        // default every keystroke that parses is written, so Esc after
        // typing would keep the half-typed value.
        .update_while_editing(false)
        .custom_parser(move |text| {
            parse_entry(&ctx, unit, bad_id, text).map(|si| unit.from_si(f64::from(si)))
        });
    if let Some((lo, hi)) = shown_range {
        drag = drag.range(lo..=hi);
    }
    let mut response = ui.add(drag);
    if response.changed() {
        *value = spec.si(shown);
    }

    let cancelled = scrub(ui, &response, &spec, id, value, before);
    if response.hovered() || response.dragged() || response.has_focus() {
        claim_keys(ui.ctx());
    }
    let mut instant = Instant {
        value,
        before: None,
    };
    if response.hovered() && !response.dragged() && !response.has_focus() {
        hover_edits(ui, &response, &spec, &mut instant);
    }
    menu(ui, &response, &spec, &mut instant);
    show_bad_entry(ui, &response, spec.bad_id);

    if (*instant.value - before).abs() > 0.0 {
        response.mark_changed();
    }
    Field {
        response,
        instant: instant.before,
        cancelled,
    }
}

/// The drag: remembers where it started; Esc puts the value back there;
/// Ctrl snaps. Returns whether Esc cancelled
/// it this frame.
fn scrub(
    ui: &Ui,
    response: &egui::Response,
    spec: &Spec,
    id: egui::Id,
    value: &mut f32,
    before: f32,
) -> bool {
    let start_id = id.with("scrub-start");
    let mut cancelled = false;
    if response.drag_started() {
        ui.data_mut(|d| d.insert_temp(start_id, before));
    }
    // egui itself aborts a drag on Esc: the frame Esc is pressed the drag
    // has stopped. Put the value back so the gesture commits nothing.
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if (response.dragged() || response.drag_stopped()) && escaped {
        *value = ui.data(|d| d.get_temp::<f32>(start_id)).unwrap_or(before);
        cancelled = true;
    } else if response.dragged() && response.changed() {
        let modifiers = ui.input(|i| i.modifiers);
        if modifiers.command {
            let step = if modifiers.shift {
                step::fine(spec.step)
            } else {
                spec.step
            };
            *value = spec.clamp(spec.si(step::snap(spec.shown(*value), step)));
        }
    }
    if response.drag_stopped() {
        ui.data_mut(|d| d.remove::<f32>(start_id));
    }
    cancelled
}

/// Keys, wheel and middle-click over a hovered field.
fn hover_edits(ui: &Ui, response: &egui::Response, spec: &Spec, instant: &mut Instant<'_>) {
    let input = hover_input(ui);
    if input.wheel != 0 {
        let shown = spec.shown(*instant.value) + f64::from(input.wheel) * spec.step;
        instant.set(spec, spec.si(shown));
    }
    let reset = input.reset || response.clicked_by(egui::PointerButton::Middle);
    if reset && let Some(default) = spec.default {
        instant.set(spec, default);
    }
    if input.negate {
        instant.set(spec, -*instant.value);
    }
    if input.copy {
        spec.copy(ui.ctx(), *instant.value);
    }
    if let Some(text) = input.paste {
        instant.paste(ui, spec, &text);
    }
}

/// What the right-click menu asked for.
enum MenuPick {
    Copy,
    Paste(String),
    Reset,
}

/// The right-click menu: Copy, Paste (the last copied field text), Reset.
fn menu(ui: &Ui, response: &egui::Response, spec: &Spec, instant: &mut Instant<'_>) {
    let mut pick = None;
    let clipboard = copied(ui.ctx());
    response.context_menu(|ui| {
        if ui.button("Copy").clicked() {
            pick = Some(MenuPick::Copy);
            ui.close();
        }
        let paste = ui.add_enabled(clipboard.is_some(), egui::Button::new("Paste"));
        let paste = match &clipboard {
            Some(text) => paste.on_hover_text(format!("Paste {text}")),
            None => paste.on_disabled_hover_text("Copy a number field first"),
        };
        if paste.clicked()
            && let Some(text) = &clipboard
        {
            pick = Some(MenuPick::Paste(text.clone()));
            ui.close();
        }
        let reset = ui.add_enabled(
            spec.default.is_some(),
            egui::Button::new("Reset to default"),
        );
        let reset = match spec.default {
            Some(d) => reset.on_hover_text(format!("Set {} (Backspace)", spec.unit.format(d, 6))),
            None => reset.on_disabled_hover_text("This field has no default"),
        };
        if reset.clicked() {
            pick = Some(MenuPick::Reset);
            ui.close();
        }
    });
    match pick {
        Some(MenuPick::Copy) => spec.copy(ui.ctx(), *instant.value),
        Some(MenuPick::Paste(text)) => instant.paste(ui, spec, &text),
        Some(MenuPick::Reset) => {
            if let Some(default) = spec.default {
                instant.set(spec, default);
            }
        }
        None => {}
    }
}

/// Outlines a field whose last entry was refused and says why, for
/// [`BAD_ENTRY_SECS`] after it.
fn show_bad_entry(ui: &Ui, response: &egui::Response, bad_id: egui::Id) {
    let Some(bad) = ui.data(|d| d.get_temp::<BadEntry>(bad_id)) else {
        return;
    };
    let left = BAD_ENTRY_SECS - (ui.input(|i| i.time) - bad.at);
    if left <= 0.0 {
        ui.data_mut(|d| d.remove::<BadEntry>(bad_id));
        return;
    }
    let error = ui.visuals().error_fg_color;
    ui.painter().rect_stroke(
        response.rect,
        ui.visuals().widgets.inactive.corner_radius,
        egui::Stroke::new(1.0, error),
        egui::StrokeKind::Inside,
    );
    egui::Tooltip::always_open(
        ui.ctx().clone(),
        response.layer_id,
        response.id.with("bad-entry-tip"),
        response,
    )
    .show(|ui| {
        ui.colored_label(error, format!("{}: {}", bad.text, bad.message));
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64(left));
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

/// Brief U-8's behaviours, driven through `egui_kittest`: each edit-seam
/// gesture commits once, or not at all.
#[cfg(test)]
mod behaviour_tests {
    use super::*;
    use crate::widgets::{Commit, precise_drag_unit};
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// The edit seam as the inspector drives it: the field is re-seeded from
    /// `source` every frame, and `source` changes only on a commit (as an
    /// authored component changes only when its intent is applied).
    struct Probe {
        source: f32,
        default: f32,
        dim: Dimension,
        commits: Vec<(f32, f32)>,
        claimed: bool,
    }

    fn harness(source: f32, default: f32, dim: Dimension) -> Harness<'static, Probe> {
        let probe = Probe {
            source,
            default,
            dim,
            commits: Vec::new(),
            claimed: false,
        };
        let mut harness = Harness::new_ui_state(
            |ui, p: &mut Probe| {
                let mut value = p.source;
                if let Commit::Done(old, new) = precise_drag_unit(
                    ui,
                    egui::Id::new("probe"),
                    &mut value,
                    p.default,
                    0.01,
                    p.dim,
                ) {
                    p.commits.push((old, new));
                    p.source = new;
                }
                p.claimed = take_key_claim(ui.ctx());
            },
            probe,
        );
        harness.run();
        harness
    }

    fn field(h: &Harness<'_, Probe>) -> egui::Rect {
        h.get_by_role(Role::SpinButton).rect()
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// Presses at the field, moves `dx` points right in steps, and (if
    /// `release`) lets go; `modifiers` are held throughout.
    fn scrub(
        h: &mut Harness<'_, Probe>,
        dx: f32,
        modifiers: egui::Modifiers,
        escape_at: Option<usize>,
    ) {
        let at = field(h).center();
        h.hover_at(at);
        h.event(egui::Event::ModifiersChanged(modifiers));
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        let steps = 10;
        for i in 1..=steps {
            if escape_at == Some(i) {
                h.event(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
            }
            h.hover_at(at + egui::vec2(dx * i as f32 / steps as f32, 0.0));
        }
        h.drop_at(at + egui::vec2(dx, 0.0));
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run();
    }

    #[test]
    fn a_drag_commits_once_on_release() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        scrub(&mut h, 50.0, egui::Modifiers::NONE, None);
        let p = h.state();
        assert_eq!(p.commits.len(), 1, "{:?}", p.commits);
        let (old, new) = p.commits[0];
        assert!(near(old, 1.0), "{:?}", p.commits);
        assert!(new > 1.3 && new < 1.6, "50 points at 0.01 m: {new}");
    }

    #[test]
    fn shift_scrubs_at_a_tenth() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        scrub(&mut h, 50.0, egui::Modifiers::SHIFT, None);
        let p = h.state();
        assert_eq!(p.commits.len(), 1, "{:?}", p.commits);
        let moved = p.commits[0].1 - 1.0;
        assert!(moved > 0.02 && moved < 0.07, "a tenth of ~0.45 m: {moved}");
    }

    #[test]
    fn escape_during_a_drag_restores_and_commits_nothing() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        scrub(&mut h, 60.0, egui::Modifiers::NONE, Some(5));
        let p = h.state();
        assert!(p.commits.is_empty(), "{:?}", p.commits);
        assert!(near(p.source, 1.0));
        let shown = h.get_by_role(Role::SpinButton).value().unwrap_or_default();
        assert!(shown.contains("1.00"), "{shown}");
    }

    #[test]
    fn ctrl_snaps_a_length_to_whole_units() {
        let mut h = harness(0.0, 0.0, Dimension::Length);
        scrub(&mut h, 130.0, egui::Modifiers::COMMAND, None);
        let p = h.state();
        assert_eq!(p.commits.len(), 1, "{:?}", p.commits);
        assert!(
            near(p.commits[0].1, 1.0),
            "1.3 m snaps to 1 m: {:?}",
            p.commits
        );
    }

    #[test]
    fn ctrl_snaps_an_angle_to_ten_degrees() {
        let mut h = harness(0.0, 0.0, Dimension::Angle);
        // 0.01 rad per point is ~0.57° per point: 40 points is ~23°.
        scrub(&mut h, 40.0, egui::Modifiers::COMMAND, None);
        let p = h.state();
        assert_eq!(p.commits.len(), 1, "{:?}", p.commits);
        assert!(near(p.commits[0].1, 20f32.to_radians()), "{:?}", p.commits);
    }

    fn hover(h: &mut Harness<'_, Probe>) {
        let at = field(h).center();
        h.hover_at(at);
        h.run();
    }

    #[test]
    fn backspace_over_a_field_resets_it_once() {
        let mut h = harness(2.0, 1.0, Dimension::Length);
        hover(&mut h);
        assert!(h.state().claimed, "a hovered field claims the keys");
        h.key_press(egui::Key::Backspace);
        h.run();
        assert_eq!(h.state().commits, vec![(2.0, 1.0)]);
    }

    #[test]
    fn minus_over_a_field_negates_it() {
        let mut h = harness(0.5, 1.0, Dimension::Length);
        hover(&mut h);
        h.key_press(egui::Key::Minus);
        h.run();
        assert_eq!(h.state().commits, vec![(0.5, -0.5)]);
    }

    #[test]
    fn keys_away_from_a_field_do_nothing_and_are_not_claimed() {
        let mut h = harness(2.0, 1.0, Dimension::Length);
        h.hover_at(egui::pos2(500.0, 500.0));
        h.run();
        h.key_press(egui::Key::Backspace);
        h.key_press(egui::Key::Minus);
        h.run();
        assert_eq!(h.state().commits, [] as [(f32, f32); 0]);
        assert!(!h.state().claimed);
    }

    #[test]
    fn ctrl_wheel_steps_by_the_snap_step() {
        let mut h = harness(0.0, 0.0, Dimension::Angle);
        hover(&mut h);
        let notch = |y: f32| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, y),
            modifiers: egui::Modifiers::COMMAND,
            phase: egui::TouchPhase::Move,
        };
        h.event_modifiers(notch(1.0), egui::Modifiers::COMMAND);
        h.run();
        h.event_modifiers(notch(-1.0), egui::Modifiers::COMMAND);
        h.event_modifiers(notch(-1.0), egui::Modifiers::COMMAND);
        h.run();
        let ten = 10f32.to_radians();
        let commits = &h.state().commits;
        assert_eq!(commits.len(), 3, "one undo step per notch: {commits:?}");
        assert!(near(commits[0].1, ten), "{commits:?}");
        assert!(near(commits[2].1, -ten), "{commits:?}");
    }

    #[test]
    fn a_plain_wheel_does_not_step() {
        let mut h = harness(0.0, 0.0, Dimension::Angle);
        hover(&mut h);
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.run();
        assert_eq!(h.state().commits, [] as [(f32, f32); 0]);
    }

    fn menu_row(h: &mut Harness<'_, Probe>, row: &str) {
        h.get_by_role(Role::SpinButton).click_secondary();
        h.run();
        h.get_by_label(row).click();
        h.run();
    }

    #[test]
    fn the_menu_copies_pastes_and_resets() {
        let mut h = harness(0.25, 1.0, Dimension::Length);
        menu_row(&mut h, "Copy");
        assert_eq!(copied(&h.ctx).as_deref(), Some("0.25 m"));
        menu_row(&mut h, "Reset to default");
        menu_row(&mut h, "Paste");
        assert_eq!(h.state().commits, vec![(0.25, 1.0), (1.0, 0.25)]);
    }

    #[test]
    fn ctrl_v_over_a_field_pastes_text_with_units() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        hover(&mut h);
        // bevy_egui sends Ctrl+V as the key plus the clipboard's text.
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::COMMAND));
        h.event(egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run();
        // One frame with both, as bevy_egui delivers them.
        h.input_mut().events.extend([
            egui::Event::Key {
                key: egui::Key::V,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("40 cm".into()),
        ]);
        h.run();
        assert_eq!(h.state().commits.len(), 1, "{:?}", h.state().commits);
        assert!(near(h.state().commits[0].1, 0.4));
    }

    /// Clicks into the field and types `text` over its contents.
    fn type_over(h: &mut Harness<'_, Probe>, text: &str) {
        h.get_by_role(Role::SpinButton).click();
        h.run();
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        // In edit mode the field is still a spin button (egui labels the
        // text edit as the drag value), so the text goes in as an event.
        h.event(egui::Event::Text(text.to_owned()));
        h.run();
    }

    #[test]
    fn escape_while_typing_keeps_the_old_value() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        type_over(&mut h, "25");
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().commits.is_empty(), "{:?}", h.state().commits);
        assert!(near(h.state().source, 1.0));
    }

    #[test]
    fn a_refused_entry_says_why() {
        let mut h = harness(1.0, 1.0, Dimension::Length);
        type_over(&mut h, "3 furlongs");
        h.key_press(egui::Key::Enter);
        h.run();
        assert!(h.state().commits.is_empty(), "{:?}", h.state().commits);
        assert!(
            h.query_by_label_contains("unknown unit `furlongs`")
                .is_some(),
            "the parser's message shows under the field"
        );
        // A good entry clears it.
        type_over(&mut h, "2");
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(h.state().commits, vec![(1.0, 2.0)]);
        assert!(h.query_by_label_contains("furlongs").is_none());
    }
}
