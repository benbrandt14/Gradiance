//! How the editor names things: the one "kind · short id" label rule every
//! user-facing body/joint reference uses (`object_label`), and workspace
//! name tags — script-labelled bodies wear their name in the viewport
//! (`(label b "crate")` — see [`WorkspaceLabels`]).
//!
//! The tag overlay is a pure read: world position → viewport via the
//! camera, one floating egui `Area` per label. No interaction, no authored
//! writes.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::shape::ShapeDef;
use gradiance_script::bridge::WorkspaceLabels;

/// Characters of a [`StableId`] shown in a label: enough to tell same-kind
/// objects apart at a glance, never the whole UUID.
pub(crate) const SHORT_ID_LEN: usize = 6;

/// A short, stable id prefix so same-kind objects are distinguishable.
///
/// Built from the id's string rather than `{:.N}` formatting: `StableId`'s
/// `Display` forwards to the UUID's, which ignores the precision flag.
pub(crate) fn short_id(id: &StableId) -> String {
    id.to_string().chars().take(SHORT_ID_LEN).collect()
}

/// The user-facing label of an object: `"box · 0e8593"`. `kind` is the
/// object's kind word (`ShapeDef::kind_label`, "hinge", "tracer", …). The
/// outliner, the joint inspector and the context menu all name objects
/// through this, so every surface agrees.
pub(crate) fn object_label(kind: &str, id: &StableId) -> String {
    format!("{kind} · {}", short_id(id))
}

/// [`object_label`] for a body: its shape's kind word, or plain "body" when
/// the shape isn't to hand (the id outlived its entity).
pub(crate) fn body_label(shape: Option<&ShapeDef>, id: &StableId) -> String {
    object_label(shape.map_or("body", ShapeDef::kind_label), id)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_id_is_a_stable_six_char_prefix() {
        let id = StableId::new();
        let short = short_id(&id);
        assert_eq!(short.chars().count(), 6);
        assert!(id.to_string().starts_with(&short));
        // Same id → same suffix (stable across frames).
        assert_eq!(short_id(&id), short);
    }

    #[test]
    fn object_label_is_kind_and_short_id_never_the_whole_uuid() {
        let id = StableId::new();
        let full = id.to_string();
        let label = object_label("box", &id);
        assert_eq!(label, format!("box · {}", &full[..SHORT_ID_LEN]));
        assert!(
            !label.contains(&full),
            "label leaked the full UUID: {label}"
        );
        assert_eq!(short_id(&id).chars().count(), SHORT_ID_LEN);
    }
}
