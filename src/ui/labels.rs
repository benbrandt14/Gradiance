//! Workspace name tags: script-labelled bodies wear their name in the
//! viewport (`(label b "crate")` — see [`WorkspaceLabels`]).
//!
//! A pure read overlay: world position → viewport via the camera, one
//! floating egui `Area` per label. No interaction, no authored writes.

use crate::core::ids::IdIndex;
use crate::domain::Body;
use crate::script::bridge::WorkspaceLabels;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Screen offset above the body center, logical pixels.
const TAG_LIFT_PX: f32 = 26.0;

/// Draws a name tag over every labelled body still present in the scene
/// (a label whose body was deleted simply stops drawing — the binding
/// stays, since undo may bring the body back).
pub fn draw_workspace_labels(
    mut contexts: EguiContexts,
    labels: Res<WorkspaceLabels>,
    index: Res<IdIndex>,
    bodies: Query<&Transform, With<Body>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Result {
    if labels.0.is_empty() {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let Ok((camera, camera_pose)) = cameras.single() else {
        return Ok(());
    };
    for (name, id) in &labels.0 {
        let Some(entity) = index.entity(*id) else {
            continue;
        };
        let Ok(transform) = bodies.get(entity) else {
            continue;
        };
        let Ok(screen) = camera.world_to_viewport(camera_pose, transform.translation) else {
            continue;
        };
        egui::Area::new(egui::Id::new(("workspace-label", name)))
            .fixed_pos(egui::pos2(screen.x, screen.y - TAG_LIFT_PX))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_black_alpha(160))
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .color(egui::Color32::from_rgb(235, 235, 220))
                                .monospace()
                                .size(12.0),
                        );
                    });
            });
    }
    Ok(())
}
