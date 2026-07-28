//! Constraint and dimension badges drawn over the sketch.
//!
//! A constraint that exists only as a row in a side panel is invisible where it
//! matters. This is what makes the sketch *readable*: every dimension shows its
//! value on the span it measures, every relationship shows a token on the
//! geometry it ties together, and the ones the solver could not satisfy show up
//! red on the canvas rather than only in a list.
//!
//! A pure read overlay, the same shape as [`crate::labels`]: world position →
//! viewport via the camera, one non-interactive egui `Area` per badge. It
//! writes nothing.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_interaction::tools::sketch_session::SketchSession;
use gradiance_sketch::annotate::{Annotation, AnnotationKind, annotations};

/// Screen offset above the anchor, logical pixels, so a badge does not sit on
/// top of the vertex or edge it describes.
const BADGE_LIFT_PX: f32 = 14.0;

/// Beyond this many badges the canvas is noise rather than information, so the
/// overlay stops drawing and the panel's list carries it instead.
const MAX_BADGES: usize = 60;

/// Draw a badge for every constraint in the open sketch.
///
/// # Errors
///
/// Propagates the egui context lookup.
pub fn draw_sketch_annotations(
    mut contexts: EguiContexts,
    session: Res<SketchSession>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    if session.is_empty() {
        return Ok(());
    }
    let placed = annotations(session.doc());
    if placed.is_empty() || placed.len() > MAX_BADGES {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    let Ok((camera, camera_pose)) = cameras.single() else {
        return Ok(());
    };
    let failed = session.failed();

    for a in &placed {
        let world = Vec3::new(a.at.x, a.at.y, 0.0);
        let Ok(screen) = camera.world_to_viewport(camera_pose, world) else {
            continue;
        };
        draw_badge(ctx, a, failed.contains(&a.index), screen);
    }
    Ok(())
}

/// One badge. Dimensions read brighter and larger than relations: a measurement
/// is something the author chose and will want to re-read, where a relation is
/// a reminder that fades into the background once you trust it.
fn draw_badge(ctx: &egui::Context, a: &Annotation, failed: bool, screen: Vec2) {
    let (fill, text_color, size) = if failed {
        (
            egui::Color32::from_rgb(120, 30, 30),
            egui::Color32::from_rgb(255, 210, 210),
            11.0,
        )
    } else if a.kind == AnnotationKind::Dimension {
        (
            egui::Color32::from_black_alpha(170),
            egui::Color32::from_rgb(255, 236, 170),
            12.0,
        )
    } else {
        (
            egui::Color32::from_black_alpha(120),
            egui::Color32::from_rgb(150, 200, 210),
            10.0,
        )
    };

    egui::Area::new(egui::Id::new(("sketch-annotation", a.index)))
        .fixed_pos(egui::pos2(screen.x, screen.y - BADGE_LIFT_PX))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .interactable(false)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(fill)
                .corner_radius(3.0)
                .inner_margin(egui::Margin::symmetric(4, 1))
                .show(ui, |ui| {
                    let mut text = egui::RichText::new(&a.text)
                        .color(text_color)
                        .monospace()
                        .size(size);
                    if failed {
                        text = text.strong();
                    }
                    ui.label(text);
                });
        });
}
