//! Precision widgets: commit-on-release number fields (the behaviours are
//! `unit_fields::number_field`'s, brief U-8) and the layout vocabulary.
//!
//! Curve-picker widgets (Lightroom-style response curves) and symbolic /
//! equation input are planned extensions of this module: both slot in as
//! new `Commit`-returning widgets without touching callers.

use crate::unit_fields::Format;
use bevy_egui::egui::{self, Ui};
use gradiance_units::{Dimension, DisplayUnit};
use std::ops::RangeInclusive;

/// Outcome of a committing widget interaction.
pub enum Commit<T> {
    /// Still idle or mid-gesture; nothing to commit.
    None,
    /// A completed edit: `(value_before_gesture, value_now)`.
    Done(T, T),
}

/// A number field on the **edit** seam ([`crate::unit_fields::number_field`]:
/// scrub, Shift = 1/10, Ctrl snaps, Esc cancels, typed maths and units,
/// Backspace / middle-click / Reset = `default`, `-` negates, Ctrl+wheel
/// steps, Copy / Paste) that commits **once per gesture**, returning the
/// pre-gesture value for the undo record:
/// - a drag commits on release, and nothing when Esc cancelled it;
/// - a typed entry commits on Enter or focus loss, and nothing on Esc;
/// - a one-step edit (a key, a wheel notch, a menu row) commits at once.
///
/// For a pure number; [`precise_drag_unit`] is the same for a quantity.
pub fn precise_drag(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
) -> Commit<f32> {
    precise_drag_unit(ui, id, value, default, speed, Dimension::Dimensionless)
}

/// [`precise_drag`] for a physical quantity of dimension `dim`. `value`,
/// `default` and `speed` are base SI; the field shows them in the unit the
/// display preference picks ([`crate::unit_fields`]: degrees or radians,
/// metres, centimetres or millimetres) with the unit after the number.
///
/// Typed entry: a bare number is in the shown unit, a number with a unit
/// converts (`25 cm` in a metre field stores `0.25`), and an entry of the
/// wrong dimension or that does not parse leaves the value as it was.
pub fn precise_drag_unit(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    dim: Dimension,
) -> Commit<f32> {
    let unit = crate::unit_fields::display(ui.ctx(), dim);
    precise_drag_in(ui, id, value, default, speed, unit)
}

/// [`precise_drag_unit`] in a fixed unit rather than the preferred one, for
/// a field whose unit is its own (a hinge motor's speed in rpm).
pub fn precise_drag_in(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    unit: DisplayUnit,
) -> Commit<f32> {
    precise_field(
        ui,
        id,
        value,
        (default, speed),
        None,
        unit,
        Format::default(),
    )
}

/// [`precise_drag`] for a whole number (a count): shown and committed
/// rounded.
pub fn precise_drag_int(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
) -> Commit<f32> {
    let unit = crate::unit_fields::display(ui.ctx(), Dimension::Dimensionless);
    precise_field(ui, id, value, (default, speed), None, unit, Format::INTEGER)
}

/// [`precise_drag_unit`] held to `range` (base SI).
pub fn precise_drag_clamped(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    default: f32,
    speed: f32,
    range: RangeInclusive<f32>,
    dim: Dimension,
) -> Commit<f32> {
    let unit = crate::unit_fields::display(ui.ctx(), dim);
    precise_field(
        ui,
        id,
        value,
        (default, speed),
        Some(range),
        unit,
        Format::default(),
    )
}

/// The edit seam's one field: `(default, speed)`, an optional range, all
/// base SI.
fn precise_field(
    ui: &mut Ui,
    id: egui::Id,
    value: &mut f32,
    (default, speed): (f32, f32),
    range: Option<RangeInclusive<f32>>,
    unit: DisplayUnit,
    format: Format,
) -> Commit<f32> {
    let start_id = id.with("gesture-start");
    let active_id = id.with("gesture-active");

    // An authored-property field only commits on *release*, so its source
    // (the component) stays at the pre-edit value for the whole gesture and
    // the caller re-seeds `*value` to it every frame. Restore the in-progress
    // value first, or the drag can never accumulate — it resets to the source
    // each frame and the field looks "frozen". (Settings fields update in
    // place, so `active` equals the source and this is a no-op for them.)
    if let Some(active) = ui.data(|d| d.get_temp::<f32>(active_id)) {
        *value = active;
    }

    let before = *value;
    let field =
        crate::unit_fields::number_field_as(ui, value, speed, range, unit, Some(default), format);
    let response = field.response;

    // A one-step edit (Backspace, `-`, Ctrl+wheel, paste, Reset,
    // middle-click) is a whole gesture: commit it now.
    if let Some(old) = field.instant {
        ui.data_mut(|d| {
            d.remove::<f32>(active_id);
            d.remove::<f32>(start_id);
        });
        if (old - *value).abs() > f32::EPSILON {
            return Commit::Done(old, *value);
        }
        return Commit::None;
    }

    // The value before this frame's change: on a drag's first frame egui
    // has already moved it.
    if response.drag_started() || response.gained_focus() {
        ui.data_mut(|d| d.insert_temp(start_id, before));
    }
    // Carry the working value across frames while the gesture is live; drop it
    // the moment it ends so a stale value can't shadow the source.
    if response.dragged() || response.has_focus() {
        ui.data_mut(|d| d.insert_temp(active_id, *value));
    } else {
        ui.data_mut(|d| d.remove::<f32>(active_id));
    }

    let finished = response.drag_stopped() || response.lost_focus();
    if finished {
        let old = ui
            .data_mut(|d| d.get_temp::<f32>(start_id))
            .unwrap_or(*value);
        ui.data_mut(|d| d.remove::<f32>(start_id));
        if (old - *value).abs() > f32::EPSILON {
            return Commit::Done(old, *value);
        }
    }
    Commit::None
}

// ---------------------------------------------------------------------------
// Layout vocabulary
// ---------------------------------------------------------------------------
//
// These exist because the crate had grown four different ways to write a
// section header, two spellings of the close button (neither of which
// rendered — see `crate::fonts`), and no shared notion of a labelled row or an
// empty state at all. Each panel had reinvented them slightly differently, so
// nothing lined up between panes.
//
// They are deliberately thin. The goal is one obvious way to say a common
// thing, not an abstraction layer over egui.

/// A section heading.
///
/// The crate previously used `RichText::strong`, `RichText::weak`,
/// `ui.heading`, and bare `CollapsingHeader` for the same job, so headings
/// changed weight from pane to pane. This is the one spelling.
///
/// Returns the `Response` so a header can still carry hover text, which
/// several of them do.
pub fn section_header(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).strong())
}

/// Explanatory text under a header or beside a control.
///
/// Small and dimmed — for the sentence that says what a section is *for*,
/// which several panes were writing as a full-weight label.
pub fn hint(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).weak().small())
}

/// The placeholder shown when a panel has nothing to display.
///
/// Five panes phrased these differently and two used a full-weight `label`,
/// so an empty panel read as either an error or a heading depending on which
/// one you were looking at.
pub fn empty_state(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).weak())
}

/// A small close / remove button.
///
/// One glyph and one size for an action that had two of each. Returns whether
/// it was clicked.
pub fn close_button(ui: &mut Ui, hover: &str) -> bool {
    ui.small_button(crate::fonts::glyph::CLOSE)
        .on_hover_text(hover)
        .clicked()
}

/// A number field on the **config** seam: the caller writes the value to a
/// settings resource (or a panel's own state) every frame it changes, so
/// there is no gesture bookkeeping. Built from its default and speed
/// ([`config_field`]); `show` draws it over an `f32`, `show_int` over any
/// integer. Every behaviour of [`crate::unit_fields::number_field`] reaches
/// it, including Backspace, a middle click and the menu's Reset to the
/// default (brief U-8b).
#[derive(Clone, Debug)]
pub struct ConfigField {
    default: Option<f32>,
    speed: f32,
    range: Option<RangeInclusive<f32>>,
    stored: DisplayUnit,
    format: Format,
}

/// A [`ConfigField`] whose reset value is `default`, scrubbing `speed` per
/// point: dimensionless, unbounded, until the builder says otherwise.
#[must_use]
pub fn config_field(default: f32, speed: f32) -> ConfigField {
    ConfigField {
        default: Some(default),
        speed,
        range: None,
        stored: DisplayUnit::canonical(Dimension::Dimensionless),
        format: Format::default(),
    }
}

impl ConfigField {
    /// No default after all (a struct the reflect grid cannot build a
    /// default of): Backspace, a middle click and Reset do nothing.
    #[must_use]
    pub fn no_default(mut self) -> Self {
        self.default = None;
        self
    }

    /// Held to `range` (in the stored unit).
    #[must_use]
    pub fn range(mut self, range: RangeInclusive<f32>) -> Self {
        self.range = Some(range);
        self
    }

    /// A quantity kept in `stored` (usually [`DisplayUnit::canonical`]; a
    /// settings angle kept in degrees passes [`DisplayUnit::DEGREES`]): the
    /// field shows and parses in the unit the display preference picks.
    /// `default`, `speed` and `range` are in `stored`.
    #[must_use]
    pub fn stored(mut self, stored: DisplayUnit) -> Self {
        self.stored = stored;
        self
    }

    /// A quantity of dimension `dim`, kept in its canonical SI unit.
    #[must_use]
    pub fn unit(self, dim: Dimension) -> Self {
        self.stored(DisplayUnit::canonical(dim))
    }

    /// Text before the number.
    #[must_use]
    pub fn prefix(mut self, prefix: &'static str) -> Self {
        self.format.prefix = prefix;
        self
    }

    /// Text after the number and its unit.
    #[must_use]
    pub fn suffix(mut self, suffix: &'static str) -> Self {
        self.format.suffix = suffix;
        self
    }

    /// Shown as `1.0e-4` (a tolerance).
    #[must_use]
    pub fn scientific(mut self) -> Self {
        self.format.scientific = true;
        self
    }

    /// Draws the field over `value`.
    pub fn show(self, ui: &mut Ui, value: &mut f32) -> egui::Response {
        crate::unit_fields::stored_drag(
            ui,
            value,
            self.range,
            self.speed,
            self.stored,
            self.default,
            self.format,
        )
    }

    /// Draws the field over a whole number: it shows no decimals and
    /// writes the rounded value.
    pub fn show_int<T: egui::emath::Numeric>(
        mut self,
        ui: &mut Ui,
        value: &mut T,
    ) -> egui::Response {
        self.format.decimals = Some(0);
        let mut v = value.to_f64() as f32;
        let response = self.show(ui, &mut v);
        if response.changed() {
            *value = T::from_f64(f64::from(v).round());
        }
        response
    }
}

/// A labelled numeric row inside an [`egui::Grid`]: label, field, `end_row`.
///
/// Generalised from the optimizer's private helper, which was already the
/// right shape and the only place in the crate where labels lined up into a
/// column. Everywhere else builds rows as ad-hoc `ui.horizontal`, so labels
/// never align — using this is what makes a panel look like the rest.
///
/// For **authored** values prefer [`precise_drag_unit`], which commits once
/// per gesture for the undo record; this is for config-seam edits, which are
/// not undoable gestures. `field` carries the default, the range and (for a
/// quantity) the stored unit.
pub fn labelled_drag(
    ui: &mut Ui,
    label: &str,
    hover: &str,
    value: &mut f32,
    field: ConfigField,
) -> bool {
    ui.label(label).on_hover_text(hover);
    let changed = field.show(ui, value).changed();
    ui.end_row();
    changed
}

/// [`labelled_drag`] for a whole number.
pub fn labelled_drag_int<T: egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    hover: &str,
    value: &mut T,
    field: ConfigField,
) -> bool {
    ui.label(label).on_hover_text(hover);
    let changed = field.show_int(ui, value).changed();
    ui.end_row();
    changed
}

/// The largest thrust the thruster field takes, either way (N).
const MAX_THRUST: f32 = 1.0e5;

/// The behavior-node kind editor widgets.
///
/// Lives here rather than beside any one host because it has three: the
/// inspector, the node context menu, and the signal list. Returns the edited
/// kind when a field changed (the caller emits one undoable
/// `PropertyEditIntent` — this widget never mutates). `salt` keeps widget ids
/// unique between host surfaces.
pub fn node_kind_editor(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &gradiance_domain::node::NodeKind,
) -> Option<gradiance_domain::node::NodeKind> {
    use gradiance_domain::node::NodeKind;
    let mut next = kind.clone();
    match &mut next {
        NodeKind::Tracer(tracer) => {
            // The edit seam: each field reports once, on release (or at
            // once for a reset), so a drag is one undo step.
            let defaults = gradiance_domain::tracer::Tracer::default();
            let mut committed = false;
            ui.horizontal(|ui| {
                ui.label("fade");
                committed |= matches!(
                    precise_drag_clamped(
                        ui,
                        ui.id().with((salt, "tracer-fade")),
                        &mut tracer.fade_secs.0,
                        defaults.fade_secs.0,
                        0.05,
                        0.0..=f32::MAX,
                        Dimension::Time,
                    ),
                    Commit::Done(..)
                );
                ui.label("size");
                committed |= matches!(
                    precise_drag_clamped(
                        ui,
                        ui.id().with((salt, "tracer-size")),
                        &mut tracer.size.0,
                        defaults.size.0,
                        0.1,
                        0.5..=20.0,
                        Dimension::Length,
                    ),
                    Commit::Done(..)
                );
            });
            ui.horizontal(|ui| {
                ui.label("pattern");
                egui::ComboBox::from_id_salt(ui.id().with((salt, "pattern")))
                    .selected_text(format!("{:?}", tracer.pattern))
                    .show_ui(ui, |ui| {
                        for p in gradiance_domain::tracer::TracePattern::ALL {
                            committed |= ui
                                .selectable_value(&mut tracer.pattern, p, format!("{p:?}"))
                                .changed();
                        }
                    });
            });
            // Mid-drag the working value is in `next` too: report only a
            // finished edit.
            if !committed {
                return None;
            }
        }
        NodeKind::Thruster(thruster) => {
            // The same edit seam as the tracer's: one undo step per drag.
            let defaults = gradiance_domain::thruster::Thruster::default();
            let mut committed = false;
            ui.horizontal(|ui| {
                ui.label("thrust");
                committed |= matches!(
                    precise_drag_clamped(
                        ui,
                        ui.id().with((salt, "thruster-thrust")),
                        &mut thruster.thrust.0,
                        defaults.thrust.0,
                        0.1,
                        -MAX_THRUST..=MAX_THRUST,
                        Dimension::Force,
                    ),
                    Commit::Done(..)
                );
                ui.label("axis");
                committed |= matches!(
                    precise_drag_clamped(
                        ui,
                        ui.id().with((salt, "thruster-angle")),
                        &mut thruster.angle.0,
                        defaults.angle.0,
                        0.01,
                        -core::f32::consts::TAU..=core::f32::consts::TAU,
                        Dimension::Angle,
                    ),
                    Commit::Done(..)
                );
            });
            if !committed {
                return None;
            }
        }
        NodeKind::Laser(laser) => {
            if !laser_editor(ui, salt, laser) {
                return None;
            }
        }
    }
    (next != *kind).then_some(next)
}

/// The laser's fields (brief N-8): its bus name, axis, range, bounce limit
/// and target. Each reports once per gesture (the edit seam); the name
/// commits on Enter or focus loss, Esc cancels. The target is set by the
/// laser tool or the `laser` operation; here it can only be cleared.
/// Returns whether a field finished an edit.
fn laser_editor(ui: &mut egui::Ui, salt: &str, laser: &mut gradiance_domain::laser::Laser) -> bool {
    use gradiance_domain::laser::{Laser, MAX_BOUNCES, MAX_RANGE};
    let defaults = Laser::default();
    let mut committed = false;
    ui.horizontal(|ui| {
        ui.label("signal");
        // The text being typed lives in egui's memory until it commits, so a
        // name is one edit, not one per keystroke.
        let id = ui.id().with((salt, "laser-name"));
        let mut text = ui
            .data_mut(|d| d.get_temp::<String>(id))
            .unwrap_or_else(|| laser.name.clone());
        let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0));
        if edit.lost_focus() {
            ui.data_mut(|d| d.remove::<String>(id));
            let name = text.trim();
            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if !cancelled && !name.is_empty() && name != laser.name {
                laser.name = name.to_owned();
                committed = true;
            }
        } else if edit.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        }
    });
    ui.horizontal(|ui| {
        ui.label("axis");
        committed |= matches!(
            precise_drag_clamped(
                ui,
                ui.id().with((salt, "laser-angle")),
                &mut laser.angle.0,
                defaults.angle.0,
                0.01,
                -core::f32::consts::TAU..=core::f32::consts::TAU,
                Dimension::Angle,
            ),
            Commit::Done(..)
        );
        ui.label("range");
        committed |= matches!(
            precise_drag_clamped(
                ui,
                ui.id().with((salt, "laser-range")),
                &mut laser.range.0,
                defaults.range.0,
                0.1,
                0.0..=MAX_RANGE,
                Dimension::Length,
            ),
            Commit::Done(..)
        );
    });
    ui.horizontal(|ui| {
        ui.label("bounces");
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let mut bounces = laser.bounces as f32;
        #[expect(clippy::cast_precision_loss, reason = "a small count")]
        let default = defaults.bounces as f32;
        if let Commit::Done(..) = precise_drag_int(
            ui,
            ui.id().with((salt, "laser-bounces")),
            &mut bounces,
            default,
            0.1,
        ) {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "clamped to the bounce cap first"
            )]
            let next = bounces.round().clamp(0.0, MAX_BOUNCES as f32) as u32;
            laser.bounces = next;
            committed = true;
        }
        ui.label("target");
        if laser.target.is_some() {
            ui.label("a body");
            if ui
                .small_button("clear")
                .on_hover_text("the hit signal reads 0 with no target")
                .clicked()
            {
                laser.target = None;
                committed = true;
            }
        } else {
            ui.weak("none");
        }
    });
    committed
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    /// Presses and releases the middle button on every number field, one
    /// frame each.
    fn middle_click_every_field<S>(h: &mut Harness<'_, S>) {
        let centres: Vec<egui::Pos2> = h
            .get_all_by_role(Role::SpinButton)
            .map(|n| n.rect().center())
            .collect();
        assert!(!centres.is_empty(), "no number field drawn");
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
    }

    /// Brief U-8b: a config field (a float and a whole number) goes back
    /// to its default on a middle click.
    #[test]
    fn a_middle_click_resets_config_fields() {
        let mut h = Harness::new_ui_state(
            |ui, (v, n): &mut (f32, u32)| {
                ui.horizontal(|ui| {
                    config_field(0.25, 0.01).range(0.0..=1.0).show(ui, v);
                    config_field(4.0, 0.2).range(1.0..=9.0).show_int(ui, n);
                });
            },
            (0.75_f32, 7_u32),
        );
        h.run();
        middle_click_every_field(&mut h);
        assert_eq!(*h.state(), (0.25, 4));
    }

    /// The tracer node's fields are on the edit seam: a middle click is one
    /// finished edit, reported once.
    #[test]
    fn a_middle_click_resets_a_tracer_field_once() {
        use gradiance_domain::node::NodeKind;
        use gradiance_domain::tracer::Tracer;
        let start = Tracer {
            fade_secs: gradiance_units::Time(9.0),
            ..Tracer::default()
        };
        let mut h = Harness::new_ui_state(
            |ui, (kind, edits): &mut (NodeKind, Vec<NodeKind>)| {
                if let Some(next) = node_kind_editor(ui, "test", kind) {
                    edits.push(next.clone());
                    *kind = next;
                }
            },
            (NodeKind::Tracer(start), Vec::new()),
        );
        h.run();
        let at = h
            .get_all_by_role(Role::SpinButton)
            .next()
            .map_or_else(egui::Pos2::default, |n| n.rect().center());
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
        let edits = &h.state().1;
        assert_eq!(edits.len(), 1, "{edits:?}");
        // Only the fade moved; the size (below its field's range) was shown,
        // never written.
        let reset = Tracer {
            fade_secs: Tracer::default().fade_secs,
            ..start
        };
        assert_eq!(edits[0], NodeKind::Tracer(reset));
    }

    /// The helpers must not panic on an empty context — they are called from
    /// every pane, including headless where there is no real UI.
    #[test]
    fn the_layout_helpers_render_headlessly() {
        // egui's own test harness: builds a real `Ui` without a window, which
        // is the path every headless panel test takes.
        egui::__run_test_ui(|ui| {
            let _ = section_header(ui, "Section");
            let _ = hint(ui, "what this is for");
            let _ = empty_state(ui, "Nothing selected");
            let _ = close_button(ui, "remove");
            egui::Grid::new("t").num_columns(2).show(ui, |ui| {
                let mut v = 1.0_f32;
                let _ = labelled_drag(
                    ui,
                    "value",
                    "hover",
                    &mut v,
                    config_field(1.0, 0.01).range(0.0..=2.0),
                );
                let mut n = 3_u32;
                let _ = labelled_drag_int(
                    ui,
                    "count",
                    "hover",
                    &mut n,
                    config_field(3.0, 0.2).range(1.0..=9.0),
                );
            });
        });
    }
}
