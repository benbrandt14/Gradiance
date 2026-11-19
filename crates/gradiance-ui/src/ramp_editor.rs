//! The **ramp editor** (brief 4b-6): a Blender ColorRamp-style gradient
//! picker for the toon falloff (`gradiance_domain::ramp::ColorRamp`).
//!
//! A gradient bar shows the ramp's tones; under it, strips show three palette
//! fills through the ramp (the tones tint by hard light, so what a body looks
//! like is not the bar itself). Stop markers sit under the bar:
//!
//! - **drag** a marker to move its stop (it may pass a neighbour: the stops
//!   re-sort and the grab follows the stop);
//! - **click** a marker to select it, then edit its position and colour in
//!   the row below;
//! - **double-click** the bar to add a stop there (it takes the tone the
//!   ramp already has, so nothing changes until you edit it);
//! - **right-click** a marker, or the minus button, to remove a stop (one
//!   always stays).
//!
//! Like [`curve_editor`](crate::curve::curve_editor) it edits the value in
//! place and reports a change; the caller owns the write seam. In Settings
//! the ramp is scene content: a settled edit is one undo step through
//! `commit_settings_edits`.

use crate::fonts::glyph;
use crate::theme::{color32, tokens};
use crate::widgets;
use bevy_egui::egui;
use gradiance_domain::appearance::Rgba;
use gradiance_domain::ramp::{ColorRamp, MAX_STOPS, RampInterp, shade};

/// Height of the tone bar.
const BAR_HEIGHT: f32 = 20.0;
/// Height of each fill-preview strip.
const PREVIEW_HEIGHT: f32 = 6.0;
/// Height of the marker row.
const MARKER_HEIGHT: f32 = 12.0;
/// Columns the bar is drawn in.
const BAR_SLICES: usize = 96;
/// How close (points, along the bar) the pointer must be to grab a marker.
const GRAB_RADIUS: f32 = 7.0;

/// Draws the ramp editor; `fills` are the preview strips' colours and
/// `default` is what the reset button restores (the look's ramp). Returns
/// whether `ramp` changed this frame.
pub fn ramp_editor(
    ui: &mut egui::Ui,
    id_salt: &str,
    ramp: &mut ColorRamp,
    fills: &[Rgba],
    default: &ColorRamp,
) -> bool {
    let id = ui.id().with(id_salt);
    let selected_id = id.with("selected");
    let mut selected = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(selected_id))
        .unwrap_or(0)
        .min(ramp.stops.len().saturating_sub(1));
    let mut changed = header(ui, ramp, &mut selected, default);
    changed |= bar(ui, id, ramp, fills, &mut selected);
    changed |= stop_row(ui, id, ramp, default, &mut selected);
    widgets::hint(
        ui,
        &format!(
            "drag a stop {m} double-click to add {m} right-click to remove",
            m = glyph::MIDDOT
        ),
    );
    ui.ctx().data_mut(|d| d.insert_temp(selected_id, selected));
    changed
}

/// The interpolation choice and the add, remove and reset buttons.
fn header(
    ui: &mut egui::Ui,
    ramp: &mut ColorRamp,
    selected: &mut usize,
    default: &ColorRamp,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for interp in RampInterp::ALL {
            let (label, hover) = match interp {
                RampInterp::Constant => ("Constant", "each tone holds until the next stop: bands"),
                RampInterp::Linear => ("Linear", "tones blend between stops: a smooth falloff"),
            };
            if ui
                .selectable_label(ramp.interp == interp, label)
                .on_hover_text(hover)
                .clicked()
                && ramp.interp != interp
            {
                ramp.interp = interp;
                changed = true;
            }
        }
        ui.separator();
        let add = ui
            .add_enabled(ramp.stops.len() < MAX_STOPS, egui::Button::new("+").small())
            .on_hover_text("add a stop halfway to the next one");
        if add.clicked() {
            let here = ramp.stops.get(*selected).map_or(0.5, |s| s.position);
            let next = ramp.stops.get(*selected + 1).map_or(1.0, |s| s.position);
            if let Some(i) = ramp.insert(f32::midpoint(here, next)) {
                *selected = i;
                changed = true;
            }
        }
        let remove = ui
            .add_enabled(
                ramp.stops.len() > 1,
                egui::Button::new(glyph::MINUS).small(),
            )
            .on_hover_text("remove the selected stop");
        if remove.clicked() && ramp.remove(*selected) {
            *selected = selected.saturating_sub(1);
            changed = true;
        }
        if ui
            .small_button(glyph::RESET)
            .on_hover_text("back to the look's ramp")
            .clicked()
            && ramp != default
        {
            *ramp = default.clone();
            *selected = 0;
            changed = true;
        }
    });
    changed
}

/// The tone bar, the fill previews and the markers, with their gestures.
fn bar(
    ui: &mut egui::Ui,
    id: egui::Id,
    ramp: &mut ColorRamp,
    fills: &[Rgba],
    selected: &mut usize,
) -> bool {
    let grab_id = id.with("grab");
    let mut changed = false;
    let width = ui.available_width().max(120.0);
    let height = BAR_HEIGHT + PREVIEW_HEIGHT * fills.len() as f32 + MARKER_HEIGHT;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
    let span = rect.x_range();
    let at = |x: f32| ((x - span.min) / span.span()).clamp(0.0, 1.0);
    let x_of = |position: f32| span.min + position * span.span();
    let nearest = |ramp: &ColorRamp, x: f32| {
        ramp.stops
            .iter()
            .enumerate()
            .map(|(i, s)| (i, (x_of(s.position) - x).abs()))
            .filter(|(_, d)| *d <= GRAB_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };

    if let Some(pointer) = response.interact_pointer_pos().or(response.hover_pos()) {
        let near = nearest(ramp, pointer.x);
        // A drag starts once the pointer has moved past egui's threshold:
        // grab the marker under the press, not under the pointer now.
        let pressed_near = ui
            .input(|i| i.pointer.press_origin())
            .and_then(|origin| nearest(ramp, origin.x));
        if response.drag_started()
            && let Some(i) = pressed_near
        {
            ui.ctx().data_mut(|d| d.insert_temp(grab_id, i));
            *selected = i;
        }
        if response.dragged()
            && let Some(i) = ui.ctx().data(|d| d.get_temp::<usize>(grab_id))
        {
            let to = at(pointer.x);
            if ramp
                .stops
                .get(i)
                .is_some_and(|s| (s.position - to).abs() > f32::EPSILON)
            {
                let moved = ramp.move_stop(i, to);
                ui.ctx().data_mut(|d| d.insert_temp(grab_id, moved));
                *selected = moved;
                changed = true;
            }
        }
        if response.double_clicked() && near.is_none() {
            if let Some(i) = ramp.insert(at(pointer.x)) {
                *selected = i;
                changed = true;
            }
        } else if response.clicked()
            && let Some(i) = near
        {
            *selected = i;
        }
        if response.secondary_clicked()
            && let Some(i) = near
            && ramp.remove(i)
        {
            *selected = (*selected).min(ramp.stops.len() - 1);
            changed = true;
        }
    }
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<usize>(grab_id));
    }

    paint(ui, rect, ramp, fills, *selected);
    changed
}

/// The selected stop's position and tone.
fn stop_row(
    ui: &mut egui::Ui,
    id: egui::Id,
    ramp: &mut ColorRamp,
    default: &ColorRamp,
    selected: &mut usize,
) -> bool {
    let mut changed = false;
    *selected = (*selected).min(ramp.stops.len().saturating_sub(1));
    let Some(stop) = ramp.stops.get(*selected).copied() else {
        return false;
    };
    egui::Grid::new(id.with("stop"))
        .num_columns(2)
        .show(ui, |ui| {
            let mut position = stop.position;
            let reset = default
                .stops
                .get(*selected)
                .map_or(stop.position, |s| s.position);
            if widgets::labelled_drag(
                ui,
                "position",
                "the lighting level (0 dark, 1 lit) where this stop's tone starts",
                &mut position,
                widgets::config_field(reset, 0.005).range(0.0..=1.0),
            ) && (position - stop.position).abs() > f32::EPSILON
            {
                *selected = ramp.move_stop(*selected, position);
                changed = true;
            }
            ui.label("tone").on_hover_text(
                "how this band tints a body: mid grey shows the fill as authored, darker multiplies, lighter brightens toward it",
            );
            let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let mut srgb = [byte(stop.color.r), byte(stop.color.g), byte(stop.color.b)];
            if egui::color_picker::color_edit_button_srgb(ui, &mut srgb).changed()
                && let Some(edited) = ramp.stops.get_mut(*selected)
            {
                let unit = |b: u8| f32::from(b) / 255.0;
                edited.color = Rgba::rgb(unit(srgb[0]), unit(srgb[1]), unit(srgb[2]));
                changed = true;
            }
            ui.end_row();
        });
    changed
}

/// Paints the tone bar, the fill previews and the stop markers.
fn paint(ui: &egui::Ui, rect: egui::Rect, ramp: &ColorRamp, fills: &[Rgba], selected: usize) {
    let t = tokens(ui.ctx());
    let painter = ui.painter_at(rect);
    let slice = rect.width() / BAR_SLICES as f32;
    let strips = std::iter::once(None).chain(fills.iter().copied().map(Some));
    let mut top = rect.top();
    for fill in strips {
        let h = if fill.is_some() {
            PREVIEW_HEIGHT
        } else {
            BAR_HEIGHT
        };
        for i in 0..BAR_SLICES {
            let level = (i as f32 + 0.5) / BAR_SLICES as f32;
            let tone = ramp.eval(level);
            let colour = fill.map_or(tone, |f| shade(f, tone));
            let x = rect.left() + i as f32 * slice;
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(slice + 0.5, h)),
                0.0,
                color32(colour),
            );
        }
        top += h;
    }
    let bar = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), top));
    painter.rect_stroke(
        bar,
        0.0,
        egui::Stroke::new(1.0, t.outline),
        egui::StrokeKind::Inside,
    );
    for (i, stop) in ramp.stops.iter().enumerate() {
        let x = rect.left() + stop.position * rect.width();
        let tip = egui::pos2(x, top + 1.0);
        let points = vec![
            tip,
            egui::pos2(x + 5.0, top + MARKER_HEIGHT - 1.0),
            egui::pos2(x - 5.0, top + MARKER_HEIGHT - 1.0),
        ];
        let stroke = if i == selected {
            egui::Stroke::new(2.0, t.accent)
        } else {
            egui::Stroke::new(1.0, t.text)
        };
        painter.add(egui::Shape::convex_polygon(
            points,
            color32(stop.color),
            stroke,
        ));
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, top)], {
            egui::Stroke::new(1.0, if i == selected { t.accent } else { t.outline })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;

    /// The editor's bar drawn alone, with where it starts and how wide it is.
    struct Bar {
        ramp: ColorRamp,
        selected: usize,
        rect: egui::Rect,
        edits: usize,
    }

    fn harness() -> Harness<'static, Bar> {
        let ramp = ColorRamp::new(
            RampInterp::Constant,
            &[
                (0.0, Rgba::rgb(0.3, 0.3, 0.3)),
                (0.5, gradiance_domain::ramp::neutral()),
            ],
        );
        // Short frames, so two clicks land within egui's double-click delay.
        let mut h = Harness::builder().with_step_dt(0.02).build_ui_state(
            |ui, bar: &mut Bar| {
                let top = ui.cursor().min;
                if super::bar(
                    ui,
                    egui::Id::new("t"),
                    &mut bar.ramp,
                    &[],
                    &mut bar.selected,
                ) {
                    bar.edits += 1;
                }
                bar.rect = egui::Rect::from_min_max(top, ui.min_rect().max);
            },
            Bar {
                ramp,
                selected: 0,
                rect: egui::Rect::NOTHING,
                edits: 0,
            },
        );
        h.run();
        h
    }

    fn at(h: &Harness<'_, Bar>, position: f32) -> egui::Pos2 {
        let r = h.state().rect;
        egui::pos2(r.left() + position * r.width(), r.center().y)
    }

    fn click(h: &mut Harness<'_, Bar>, pos: egui::Pos2, button: egui::PointerButton) {
        h.hover_at(pos);
        h.run();
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            h.run();
        }
    }

    #[test]
    fn dragging_a_marker_moves_its_stop_and_selects_it() {
        let mut h = harness();
        let from = at(&h, 0.5);
        h.drag_at(from);
        h.run();
        for step in 1..=6 {
            h.hover_at(from + egui::vec2(step as f32 * 10.0, 0.0));
            h.run();
        }
        let to = at(&h, 0.5) + egui::vec2(60.0, 0.0);
        h.drop_at(to);
        h.run();
        let bar = h.state();
        assert_eq!(bar.selected, 1);
        let expected = (to.x - bar.rect.left()) / bar.rect.width();
        assert!(
            (bar.ramp.stops[1].position - expected).abs() < 0.02,
            "{} vs {expected}",
            bar.ramp.stops[1].position
        );
        assert!(bar.edits > 0);
    }

    #[test]
    fn double_click_adds_a_stop_and_right_click_removes_one() {
        let mut h = harness();
        let pos = at(&h, 0.8);
        h.hover_at(pos);
        h.run();
        for _ in 0..2 {
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        h.run();
        assert_eq!(h.state().ramp.stops.len(), 3, "added");
        let added = h.state().ramp.stops[2];
        assert!((added.position - 0.8).abs() < 0.02);
        assert_eq!(
            added.color,
            gradiance_domain::ramp::neutral(),
            "takes its tone"
        );
        let first = at(&h, 0.0);
        click(&mut h, first, egui::PointerButton::Secondary);
        assert_eq!(h.state().ramp.stops.len(), 2, "removed");
    }
}
