//! The **curve editor**: a Lightroom-style response curve over the domain
//! math that already existed.
//!
//! [`Curve`] — control points, linear or monotone-cubic interpolation,
//! `eval`, serde, `Reflect` — has been in `gradiance-domain` and tested since
//! the signal layer landed, and [`SignalBinding::transfer`] has always applied
//! it. What was missing was any way to author one: every construction site
//! wrote `curve: None`, so the feature was unreachable. This module is that
//! missing widget and nothing more — it edits a `Curve` in place and reports
//! whether it changed. Deciding what to do with the change (emit an intent,
//! write a settings resource) stays with the caller, because the three call
//! sites sit on different write seams.
//!
//! # Interaction
//!
//! - **Drag** a point to move it. The two ends are pinned in `x` (the domain is
//!   always `[0, 1]`) but free in `y`, so you can lift the floor or drop the
//!   ceiling. Interior points are clamped between their neighbours, which is
//!   what keeps `points` ascending in `x` — the invariant [`Curve::eval`]
//!   relies on, enforced here rather than checked later.
//! - **Double-click** empty space to insert a point there.
//! - **Right-click** a point to delete it (never below two).
//!
//! The drag grab is stored in egui's temp memory keyed by the widget id, so the
//! editor stays a plain function the caller can drop anywhere without threading
//! state through — the same shape as the rest of `widgets.rs`.
//!
//! [`SignalBinding::transfer`]: gradiance_domain::signal::SignalBinding::transfer

use bevy::math::Vec2;
use bevy_egui::egui;
use gradiance_domain::signal::{Curve, CurveInterp};

/// Height of the editor's square-ish plot area.
const EDITOR_HEIGHT: f32 = 160.0;

/// How close (screen px) the pointer must be to grab or delete a point.
const GRAB_RADIUS: f32 = 10.0;

/// Samples used to draw the curve itself. Enough that a monotone cubic reads
/// as smooth at the editor's size without being wasteful.
const CURVE_SAMPLES: usize = 96;

/// The smallest gap in `x` an interior point keeps from its neighbours, so
/// dragging one onto another cannot produce a zero-width segment.
const MIN_GAP: f32 = 0.005;

/// Draws the curve editor for `curve`, returning `true` when the user changed
/// it this frame.
///
/// `id_salt` distinguishes multiple editors in one pass (each binding row has
/// its own).
pub fn curve_editor(ui: &mut egui::Ui, id_salt: &str, curve: &mut Curve) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for interp in [CurveInterp::Linear, CurveInterp::Smooth] {
            let label = match interp {
                CurveInterp::Linear => "Linear",
                CurveInterp::Smooth => "Smooth",
            };
            if ui
                .selectable_label(curve.interp == interp, label)
                .on_hover_text(match interp {
                    CurveInterp::Linear => "straight segments between points",
                    CurveInterp::Smooth => "monotone cubic — smooth, never overshoots",
                })
                .clicked()
                && curve.interp != interp
            {
                curve.interp = interp;
                changed = true;
            }
        }
        if ui
            .small_button(crate::fonts::glyph::RESET)
            .on_hover_text("back to the identity line (no reshaping)")
            .clicked()
            && *curve != Curve::default()
        {
            *curve = Curve::default();
            changed = true;
        }
    });

    let id = ui.id().with(id_salt);
    let response = egui_plot::Plot::new(id)
        .height(EDITOR_HEIGHT)
        .data_aspect(1.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .show_axes([true, true])
        .show_grid(true)
        .default_x_bounds(0.0, 1.0)
        .default_y_bounds(0.0, 1.0)
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(0.0..=1.0);
            plot_ui.set_plot_bounds_y(0.0..=1.0);
            draw(plot_ui, curve);
            interact(plot_ui, id, curve)
        });
    changed |= response.inner;

    ui.label(
        egui::RichText::new("drag to shape · double-click to add · right-click to remove")
            .weak()
            .small(),
    );
    changed
}

/// Paints the reference diagonal, the sampled curve, and the control points.
fn draw(plot_ui: &mut egui_plot::PlotUi<'_>, curve: &Curve) {
    // The identity, so you can see at a glance which way the curve bends.
    plot_ui.line(
        egui_plot::Line::new("identity", vec![[0.0, 0.0], [1.0, 1.0]])
            .color(egui::Color32::from_gray(70))
            .width(1.0),
    );
    let samples: Vec<[f64; 2]> = (0..=CURVE_SAMPLES)
        .map(|i| {
            let x = i as f32 / CURVE_SAMPLES as f32;
            [f64::from(x), f64::from(curve.eval(x))]
        })
        .collect();
    plot_ui.line(
        egui_plot::Line::new("curve", samples)
            .color(egui::Color32::from_rgb(120, 200, 255))
            .width(2.0),
    );
    let points: Vec<[f64; 2]> = curve
        .points
        .iter()
        .map(|p| [f64::from(p.x), f64::from(p.y)])
        .collect();
    plot_ui.points(
        egui_plot::Points::new("points", points)
            .radius(4.0)
            .color(egui::Color32::from_rgb(255, 220, 140)),
    );
}

/// Handles this frame's pointer work, returning whether the curve changed.
///
/// Split from [`draw`] so the mutation is one function: everything that can
/// reorder or resize `points` happens here, which is where the ascending-`x`
/// invariant is maintained.
fn interact(plot_ui: &mut egui_plot::PlotUi<'_>, id: egui::Id, curve: &mut Curve) -> bool {
    let response = plot_ui.response().clone();
    let Some(cursor) = plot_ui.pointer_coordinate() else {
        // Pointer left the plot — drop any grab so it doesn't resume later.
        plot_ui.ctx().data_mut(|d| d.remove::<usize>(id));
        return false;
    };
    let cursor_screen = plot_ui.screen_from_plot(cursor);
    let nearest = nearest_point(plot_ui, curve, cursor_screen);

    if response.drag_started()
        && let Some(index) = nearest
    {
        plot_ui.ctx().data_mut(|d| d.insert_temp(id, index));
    }
    if response.drag_stopped() || !response.dragged() {
        if response.drag_stopped() {
            plot_ui.ctx().data_mut(|d| d.remove::<usize>(id));
        }
    } else if let Some(index) = plot_ui.ctx().data(|d| d.get_temp::<usize>(id)) {
        return drag_point(curve, index, cursor);
    }

    if response.double_clicked() && nearest.is_none() {
        insert_point(curve, cursor);
        return true;
    }
    if response.secondary_clicked()
        && let Some(index) = nearest
        && curve.points.len() > 2
    {
        curve.points.remove(index);
        return true;
    }
    false
}

/// The index of the control point under the pointer, if any is within
/// [`GRAB_RADIUS`] — measured in **screen** space, so the hit area is the same
/// size wherever the point sits.
fn nearest_point(
    plot_ui: &egui_plot::PlotUi<'_>,
    curve: &Curve,
    cursor_screen: egui::Pos2,
) -> Option<usize> {
    curve
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let screen =
                plot_ui.screen_from_plot(egui_plot::PlotPoint::new(f64::from(p.x), f64::from(p.y)));
            (i, screen.distance(cursor_screen))
        })
        .filter(|(_, d)| *d <= GRAB_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Moves point `index` to the cursor, clamped so the points stay ascending in
/// `x` and inside the unit box. The first and last points keep their `x`: the
/// curve's domain is the whole normalized input, so shrinking it would silently
/// turn the ends into flat holds.
fn drag_point(curve: &mut Curve, index: usize, cursor: egui_plot::PlotPoint) -> bool {
    let last = curve.points.len().saturating_sub(1);
    let Some(point) = curve.points.get(index).copied() else {
        return false;
    };
    let x = if index == 0 {
        0.0
    } else if index == last {
        1.0
    } else {
        let lo = curve.points[index - 1].x + MIN_GAP;
        let hi = curve.points[index + 1].x - MIN_GAP;
        (cursor.x as f32).clamp(lo.min(hi), hi.max(lo))
    };
    let y = (cursor.y as f32).clamp(0.0, 1.0);
    let moved = Vec2::new(x, y);
    if moved == point {
        return false;
    }
    curve.points[index] = moved;
    true
}

/// Inserts a control point at the cursor, in `x` order.
fn insert_point(curve: &mut Curve, cursor: egui_plot::PlotPoint) {
    let x = (cursor.x as f32).clamp(0.0, 1.0);
    let y = (cursor.y as f32).clamp(0.0, 1.0);
    let at = curve
        .points
        .iter()
        .position(|p| p.x > x)
        .unwrap_or(curve.points.len());
    curve.points.insert(at, Vec2::new(x, y));
}

#[cfg(test)]
mod tests {
    use super::{drag_point, insert_point};
    use bevy::math::Vec2;
    use gradiance_domain::signal::Curve;

    fn at(x: f64, y: f64) -> egui_plot::PlotPoint {
        egui_plot::PlotPoint::new(x, y)
    }

    #[test]
    fn inserting_keeps_the_points_ascending_in_x() {
        let mut curve = Curve::default();
        insert_point(&mut curve, at(0.5, 0.9));
        insert_point(&mut curve, at(0.2, 0.1));
        let xs: Vec<f32> = curve.points.iter().map(|p| p.x).collect();
        assert_eq!(xs, [0.0, 0.2, 0.5, 1.0]);
        // And the curve is evaluable through the new shape.
        assert!((curve.eval(0.2) - 0.1).abs() < 1e-5);
    }

    /// The end points anchor the domain: dragging one sideways must not shrink
    /// `[0, 1]`, or the curve silently becomes a flat hold outside the new span.
    #[test]
    fn the_end_points_are_pinned_in_x_but_free_in_y() {
        let mut curve = Curve::default();
        assert!(drag_point(&mut curve, 0, at(0.4, 0.3)));
        assert_eq!(curve.points[0], Vec2::new(0.0, 0.3));
        assert!(drag_point(&mut curve, 1, at(0.6, 0.8)));
        assert_eq!(curve.points[1], Vec2::new(1.0, 0.8));
    }

    #[test]
    fn an_interior_point_cannot_cross_its_neighbours() {
        let mut curve = Curve {
            points: vec![Vec2::ZERO, Vec2::new(0.5, 0.5), Vec2::ONE],
            ..Curve::default()
        };
        // Dragged far past the right neighbour → clamped just short of it.
        drag_point(&mut curve, 1, at(9.0, 0.5));
        assert!(curve.points[1].x < curve.points[2].x);
        // Dragged far past the left neighbour → clamped just past it.
        drag_point(&mut curve, 1, at(-9.0, 0.5));
        assert!(curve.points[1].x > curve.points[0].x);
        let xs: Vec<f32> = curve.points.iter().map(|p| p.x).collect();
        assert!(
            xs.windows(2).all(|w| w[0] < w[1]),
            "still ascending: {xs:?}"
        );
    }

    #[test]
    fn y_is_clamped_to_the_unit_range() {
        let mut curve = Curve::default();
        drag_point(&mut curve, 0, at(0.0, 5.0));
        drag_point(&mut curve, 1, at(1.0, -5.0));
        assert!((curve.points[0].y - 1.0).abs() < 1e-6);
        assert!(curve.points[1].y.abs() < 1e-6);
    }

    #[test]
    fn a_drag_that_moves_nothing_reports_no_change() {
        let mut curve = Curve::default();
        assert!(
            !drag_point(&mut curve, 0, at(0.0, 0.0)),
            "same position is not an edit — otherwise every hover would record one"
        );
        assert!(
            !drag_point(&mut curve, 99, at(0.5, 0.5)),
            "index out of range"
        );
    }
}
