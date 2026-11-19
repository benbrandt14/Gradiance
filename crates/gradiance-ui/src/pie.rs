//! Pie menus (brief U-9): a radial menu of up to eight operations, opened by
//! a key held over the canvas.
//!
//! **The tools pie** is [`TOOLS_PIE`], one table of operation ids as
//! `interaction::input::KEY_OPS` is: each slice reads its label and chord
//! from the registry and invokes its operation through [`InvokeOp`], so a
//! slice is a menu row in another shape. Slices go clockwise from the top.
//!
//! Gestures (`Q`, a free key):
//!
//! - hold Q over the canvas, move toward a slice, release: that slice runs;
//! - hold, move out and back into the centre, release: nothing runs;
//! - tap Q (release without leaving the centre): the pie stays open; a click
//!   on a slice runs it, a click in the centre, Esc or Q again closes it.
//!
//! The pie never opens while a text field has the keyboard, over a panel, or
//! with a modifier held (Ctrl+Q is not Q).

use crate::widget_registry;
use bevy::prelude::*;
use bevy_egui::egui;
use gradiance_op::{InvokeOp, OpRegistry};
use std::f32::consts::TAU;

/// The tools pie: operation ids, clockwise from the top (at most eight).
pub const TOOLS_PIE: &[&str] = &[
    "tool.select",
    "tool.box",
    "tool.circle",
    "tool.polygon",
    "tool.cut",
    "tool.spring",
    "tool.hinge",
    "tool.drag",
];

/// The most slices a pie holds: one per compass direction.
pub const MAX_SLICES: usize = 8;
/// Inside this radius (points) the pointer selects nothing.
const DEAD_ZONE: f32 = 24.0;
/// Where the slice labels sit, from the centre (points).
const LABEL_RADIUS: f32 = 92.0;

/// One slice: the operation it runs and what it reads as.
#[derive(Debug, Clone, PartialEq)]
pub struct PieSlice {
    /// The operation's id.
    pub id: &'static str,
    /// The menu path's last part, else the Lisp name.
    pub label: &'static str,
    /// The operation's chord, if it declares one.
    pub chord: Option<&'static str>,
}

/// The slices for `ids`, from the registry. An id the registry lacks is
/// left out (and logged); more than [`MAX_SLICES`] are cut.
pub fn pie_slices(registry: &OpRegistry, ids: &[&'static str]) -> Vec<PieSlice> {
    ids.iter()
        .filter_map(|id| {
            let entry = registry.get(id);
            if entry.is_none() {
                warn!("pie slice `{id}` is not a registered operation");
            }
            entry
        })
        .take(MAX_SLICES)
        .map(|entry| PieSlice {
            id: entry.meta.id,
            label: entry
                .meta
                .menu
                .and_then(|path| path.rsplit('/').next())
                .unwrap_or(entry.meta.lisp),
            chord: entry.meta.chord,
        })
        .collect()
}

/// The slice an offset from the centre points at, of `count` slices laid
/// clockwise from the top; `None` inside the dead zone. Screen y grows down.
pub fn slice_at(offset: egui::Vec2, count: usize) -> Option<usize> {
    if count == 0 || offset.length() < DEAD_ZONE {
        return None;
    }
    // 0 at the top, growing clockwise on screen.
    let angle = offset.x.atan2(-offset.y).rem_euclid(TAU);
    let step = TAU / count as f32;
    Some(((angle / step).round() as usize) % count)
}

/// The direction of slice `i` of `count`, clockwise from the top.
fn direction(i: usize, count: usize) -> egui::Vec2 {
    let angle = TAU * i as f32 / count.max(1) as f32;
    egui::vec2(angle.sin(), -angle.cos())
}

/// The pie's state: editor state, never saved.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PieMenu {
    /// Where it opened, while it is open.
    pub centre: Option<egui::Pos2>,
    /// The pointer left the dead zone during this hold.
    moved: bool,
    /// Q was released inside the centre: open until a click.
    sticky: bool,
}

/// What the host tells the pie about this frame.
#[derive(Debug, Clone, Copy)]
pub struct PieInput {
    /// The scene area (the pie opens only over it).
    pub canvas: egui::Rect,
    /// The pointer is over a panel or window.
    pub over_ui: bool,
}

/// The pie as a leaf renderer: opens on Q over the canvas, tracks the
/// pointer, draws the slices, and returns the invocation chosen.
pub fn pie_ui(
    ctx: &egui::Context,
    state: &mut PieMenu,
    slices: &[PieSlice],
    input: PieInput,
) -> Option<InvokeOp> {
    let (pressed, released, escape, click, pointer, plain) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Q),
            i.key_released(egui::Key::Q),
            i.key_pressed(egui::Key::Escape),
            i.pointer.primary_clicked(),
            i.pointer.latest_pos(),
            i.modifiers.is_none(),
        )
    });
    let Some(centre) = state.centre else {
        let typing = ctx.egui_wants_keyboard_input();
        if pressed
            && plain
            && !typing
            && !input.over_ui
            && let Some(at) = pointer.filter(|p| input.canvas.contains(*p))
        {
            *state = PieMenu {
                centre: Some(at),
                ..PieMenu::default()
            };
            // Drawn from the next frame: this one's key is consumed by the
            // open itself.
        }
        return None;
    };
    let offset = pointer.map_or(egui::Vec2::ZERO, |p| p - centre);
    let hovered = slice_at(offset, slices.len());
    if hovered.is_some() {
        state.moved = true;
    }
    let chosen = |i: Option<usize>| -> Option<InvokeOp> {
        i.and_then(|i| slices.get(i))
            .map(|slice| InvokeOp::new(slice.id))
    };
    let mut close = escape || (state.sticky && pressed);
    let mut out = None;
    if released && !state.sticky {
        if hovered.is_some() {
            out = chosen(hovered);
            close = true;
        } else if state.moved {
            close = true;
        } else {
            state.sticky = true;
        }
    } else if click && state.sticky {
        out = chosen(hovered);
        close = true;
    }
    if close {
        *state = PieMenu::default();
        return out;
    }
    draw(ctx, centre, slices, hovered, pointer);
    out
}

/// Paints the pie: a ring at the centre, a pointer to the hovered slice,
/// and a plate per slice. An interactive area under it keeps the canvas's
/// tools from reacting to the pointer while it is open.
fn draw(
    ctx: &egui::Context,
    centre: egui::Pos2,
    slices: &[PieSlice],
    hovered: Option<usize>,
    pointer: Option<egui::Pos2>,
) {
    let tokens = crate::theme::tokens(ctx);
    let reach = LABEL_RADIUS + 70.0;
    let area = egui::Area::new(egui::Id::new("pie-menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(centre - egui::vec2(reach, reach))
        .show(ctx, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(2.0 * reach, 2.0 * reach), egui::Sense::click());
            let painter = ui.painter_at(rect);
            painter.circle(
                centre,
                DEAD_ZONE,
                tokens.window,
                egui::Stroke::new(1.5, tokens.outline),
            );
            if let (Some(i), Some(p)) = (hovered, pointer) {
                let dir = direction(i, slices.len());
                painter.line_segment(
                    [centre + dir * DEAD_ZONE, p],
                    egui::Stroke::new(2.0, tokens.accent),
                );
            }
            for (i, slice) in slices.iter().enumerate() {
                let at = centre + direction(i, slices.len()) * LABEL_RADIUS;
                let text = match slice.chord {
                    Some(chord) => format!("{}  {chord}", slice.label),
                    None => slice.label.to_owned(),
                };
                let lit = hovered == Some(i);
                let font = egui::TextStyle::Button.resolve(ui.style());
                let color = if lit { tokens.text_strong } else { tokens.text };
                let galley = painter.layout_no_wrap(text, font, color);
                let plate =
                    egui::Rect::from_center_size(at, galley.size() + egui::vec2(16.0, 10.0));
                let fill = if lit { tokens.selected } else { tokens.widget };
                painter.rect(
                    plate,
                    4.0,
                    fill,
                    egui::Stroke::new(1.0, tokens.border),
                    egui::StrokeKind::Outside,
                );
                painter.galley(plate.center() - galley.size() / 2.0, galley, color);
                widget_registry::publish_target(&format!("pie:{}", slice.label), plate);
            }
        });
    widget_registry::publish_rect("pie", area.response.rect);
}

/// Draws the tools pie in the app and writes what it chose.
pub fn tools_pie(
    mut contexts: bevy_egui::EguiContexts,
    mut state: ResMut<PieMenu>,
    registry: Res<OpRegistry>,
    panels: Res<crate::PanelRects>,
    mut invoke: MessageWriter<InvokeOp>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let slices = pie_slices(&registry, TOOLS_PIE);
    let input = PieInput {
        canvas: panels.scene_rect(ctx.viewport_rect()),
        over_ui: ctx.is_pointer_over_egui(),
    };
    if let Some(invocation) = pie_ui(ctx, &mut state, &slices, input) {
        invoke.write(invocation);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_go_clockwise_from_the_top() {
        let up = egui::vec2(0.0, -50.0);
        let right = egui::vec2(50.0, 0.0);
        let down = egui::vec2(0.0, 50.0);
        let left = egui::vec2(-50.0, 0.0);
        assert_eq!(slice_at(up, 8), Some(0));
        assert_eq!(slice_at(egui::vec2(40.0, -40.0), 8), Some(1));
        assert_eq!(slice_at(right, 8), Some(2));
        assert_eq!(slice_at(down, 8), Some(4));
        assert_eq!(slice_at(left, 8), Some(6));
        assert_eq!(slice_at(egui::vec2(-40.0, -40.0), 8), Some(7));
        // Four slices: N, E, S, W.
        assert_eq!(slice_at(left, 4), Some(3));
    }

    #[test]
    fn the_centre_selects_nothing() {
        assert_eq!(slice_at(egui::vec2(5.0, 5.0), 8), None);
        assert_eq!(slice_at(egui::vec2(50.0, 0.0), 0), None);
    }

    #[test]
    fn a_direction_points_at_its_own_slice() {
        for count in 1..=MAX_SLICES {
            for i in 0..count {
                assert_eq!(
                    slice_at(direction(i, count) * LABEL_RADIUS, count),
                    Some(i),
                    "{i} of {count}"
                );
            }
        }
    }

    #[test]
    fn the_tools_pie_fits_a_pie() {
        assert!(TOOLS_PIE.len() <= MAX_SLICES);
    }
}
