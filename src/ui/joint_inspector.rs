//! The joint inspector: configure the selected joint's limits, motor, and
//! collision flag.
//!
//! Shown when a joint is selected (body selection is cleared then). Every
//! edit builds a new [`JointDef`] and emits a `PropertyEditIntent`
//! carrying [`PropertyValue::Joint`] — the same undoable path body edits
//! use, so joint configuration composes with undo/redo for free.

use crate::command::intent::{DeleteJointIntent, PropertyEditIntent};
use crate::command::property::{PropertyChange, PropertyValue};
use crate::core::ids::{IdIndex, StableId};
use crate::core::units::PosRot;
use crate::domain::joint::{
    DEFAULT_SPRING_DAMPING, DEFAULT_SPRING_STIFFNESS, JointDef, JointKind, MotorDef,
};
use crate::interaction::selection::SelectedJoint;
use crate::ui::widgets::{Commit, precise_drag};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Renders the joint inspector for the currently selected joint.
pub fn joint_inspector(
    mut contexts: EguiContexts,
    selected: Res<SelectedJoint>,
    joints: Query<(&StableId, &JointDef)>,
    transforms: Query<&Transform>,
    index: Res<IdIndex>,
    mut edits: MessageWriter<PropertyEditIntent>,
    mut deletes: MessageWriter<DeleteJointIntent>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(entity) = selected.0 else {
        return Ok(());
    };
    let Ok((&id, def)) = joints.get(entity) else {
        return Ok(());
    };

    // Edits accumulate into `next`; a single change emits one intent.
    let old = def.clone();
    let mut next = old.clone();
    let mut changed = false;

    egui::Window::new("Joint")
        .default_width(240.0)
        .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(kind_name(&def.kind)).strong());
            ui.label(
                egui::RichText::new(match def.body_b {
                    Some(b) => format!("{:.8} ↔ {b:.8}", def.body_a),
                    None => format!("{:.8} ↔ world pin", def.body_a),
                })
                .weak(),
            );
            ui.separator();

            // Sensor readouts: the joint's live geometry (its read-only ports).
            if let Some((length, angle)) = joint_geometry(def, &transforms, &index) {
                ui.label(egui::RichText::new("Sensors").strong());
                ui.horizontal(|ui| {
                    ui.label("length");
                    ui.monospace(format!("{length:.1} px"));
                });
                if matches!(def.kind, JointKind::Hinge { .. }) {
                    ui.horizontal(|ui| {
                        ui.label("angle");
                        ui.monospace(format!("{:.1}°", angle.to_degrees()));
                    });
                }
                ui.separator();
            }

            // Struts collide by default and are governed by the collision-layer
            // menu, so they don't carry a per-joint collide toggle.
            if !matches!(def.kind, JointKind::Spring { .. }) {
                let mut collide = def.common.collide_connected;
                if ui
                    .checkbox(&mut collide, "connected bodies collide")
                    .changed()
                {
                    next.common.collide_connected = collide;
                    changed = true;
                }
            }

            changed |= configure_kind(ui, &def.kind, &mut next);

            ui.separator();
            if ui.button("Delete joint").clicked() {
                deletes.write(DeleteJointIntent { id });
            }
        });

    if changed && next != old {
        edits.write(PropertyEditIntent {
            changes: vec![PropertyChange {
                id,
                old: PropertyValue::Joint(old),
                new: PropertyValue::Joint(next),
            }],
        });
    }
    Ok(())
}

/// The joint's live geometry — anchor-to-anchor `length` (px) and the relative
/// `angle` (radians) of its bodies — the read-only sensor ports of a joint.
/// `None` if an endpoint can't be resolved this frame.
fn joint_geometry(
    def: &JointDef,
    transforms: &Query<&Transform>,
    index: &IdIndex,
) -> Option<(f32, f32)> {
    let pose_a = index
        .entity(def.body_a)
        .and_then(|e| transforms.get(e).ok())
        .map(PosRot::from_transform)?;
    let world_a = def.anchor_world(pose_a.pos, pose_a.rot);
    let (world_b, rot_b) = match def.body_b {
        Some(id) => {
            let pose_b = index
                .entity(id)
                .and_then(|e| transforms.get(e).ok())
                .map(PosRot::from_transform)?;
            (
                pose_b.pos + Vec2::from_angle(pose_b.rot).rotate(def.anchor_b),
                pose_b.rot,
            )
        }
        None => (def.anchor_b, 0.0), // world pin
    };
    Some((world_a.distance(world_b), rot_b - pose_a.rot))
}

/// Renders the kind-specific config controls, writing any edit into `next.kind`.
/// Returns whether something changed.
fn configure_kind(ui: &mut egui::Ui, kind: &JointKind, next: &mut JointDef) -> bool {
    let mut changed = false;
    match kind {
        JointKind::Hinge { limits, motor } => {
            if let Some(k) = limit_section(ui, *limits, "angle limits (deg)", true) {
                next.kind = JointKind::Hinge {
                    limits: k,
                    motor: *motor,
                };
                changed = true;
            }
            if let Some(m) = motor_section(ui, *motor, "rad/s", "torque") {
                // Re-read limits from `next` in case both changed.
                let cur_limits = current_limits(&next.kind);
                next.kind = JointKind::Hinge {
                    limits: cur_limits,
                    motor: m,
                };
                changed = true;
            }
        }
        JointKind::Slider {
            axis,
            limits,
            motor,
        } => {
            if let Some(k) = limit_section(ui, *limits, "travel limits (px)", false) {
                next.kind = JointKind::Slider {
                    axis: *axis,
                    limits: k,
                    motor: *motor,
                };
                changed = true;
            }
            if let Some(m) = motor_section(ui, *motor, "px/s", "force") {
                let cur_limits = current_limits(&next.kind);
                next.kind = JointKind::Slider {
                    axis: *axis,
                    limits: cur_limits,
                    motor: m,
                };
                changed = true;
            }
        }
        JointKind::Spring {
            rest_length,
            stiffness,
            damping,
            range,
        } => {
            if let Some(new_kind) = spring_section(ui, *rest_length, *stiffness, *damping, *range) {
                next.kind = new_kind;
                changed = true;
            }
        }
    }
    changed
}

/// A human label for a joint kind, shared with the context menu.
pub fn kind_name(kind: &JointKind) -> &'static str {
    match kind {
        JointKind::Hinge { .. } => "Hinge (revolute)",
        JointKind::Slider { .. } => "Prismatic (slider)",
        JointKind::Spring { .. } => "Strut (spring-damper)",
    }
}

fn current_limits(kind: &JointKind) -> Option<[f32; 2]> {
    match kind {
        JointKind::Hinge { limits, .. } | JointKind::Slider { limits, .. } => *limits,
        JointKind::Spring { .. } => None,
    }
}

/// Limits UI: a toggle plus min/max drags. `degrees` shows/edits in
/// degrees but stores radians.
///
/// Returns `Some(new)` only on the frame something changed; the inner
/// option is the new limits (`None` = limits disabled).
#[allow(clippy::option_option)]
fn limit_section(
    ui: &mut egui::Ui,
    current: Option<[f32; 2]>,
    label: &str,
    degrees: bool,
) -> Option<Option<[f32; 2]>> {
    let mut enabled = current.is_some();
    let mut result = None;
    if ui.checkbox(&mut enabled, label).changed() {
        result = Some(if enabled {
            Some(if degrees {
                [-90_f32.to_radians(), 90_f32.to_radians()]
            } else {
                [-100.0, 100.0]
            })
        } else {
            None
        });
    }
    if let Some([min, max]) = current {
        let scale = if degrees { 1_f32.to_degrees() } else { 1.0 };
        let (mut lo, mut hi) = (min * scale, max * scale);
        ui.horizontal(|ui| {
            ui.label("min");
            let clo = precise_drag(ui, egui::Id::new("jlim-lo"), &mut lo, 0.0, 1.0);
            ui.label("max");
            let chi = precise_drag(ui, egui::Id::new("jlim-hi"), &mut hi, 0.0, 1.0);
            if matches!(clo, Commit::Done(..)) || matches!(chi, Commit::Done(..)) {
                result = Some(Some([lo / scale, hi / scale]));
            }
        });
    }
    result
}

/// Motor UI: toggle plus parameters.
///
/// Returns `Some(new)` only on the frame something changed; the inner
/// option is the new motor (`None` = motor removed).
#[allow(clippy::option_option)]
fn motor_section(
    ui: &mut egui::Ui,
    current: Option<MotorDef>,
    vel_unit: &str,
    effort_label: &str,
) -> Option<Option<MotorDef>> {
    let mut enabled = current.is_some();
    let mut result = None;
    if ui.checkbox(&mut enabled, "motor").changed() {
        result = Some(enabled.then(MotorDef::default));
    }
    if let Some(mut m) = current {
        ui.horizontal(|ui| {
            ui.label(format!("target {vel_unit}"));
            if let Commit::Done(..) = precise_drag(
                ui,
                egui::Id::new("jm-vel"),
                &mut m.target_velocity,
                2.0,
                0.1,
            ) {
                result = Some(Some(m));
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("max {effort_label}"));
            if let Commit::Done(..) = precise_drag(
                ui,
                egui::Id::new("jm-force"),
                &mut m.max_force,
                1.0e7,
                1.0e5,
            ) {
                result = Some(Some(m));
            }
        });
        let mut osc = m.oscillate;
        if ui.checkbox(&mut osc, "oscillate at limits").changed() {
            m.oscillate = osc;
            result = Some(Some(m));
        }
        let mut powered = m.enabled;
        if ui.checkbox(&mut powered, "powered").changed() {
            m.enabled = powered;
            result = Some(Some(m));
        }
    }
    result
}

/// Strut config: rest length, spring constant, damping, and an optional hard
/// range clamp (off by default = unbounded). The scalars a future curve editor
/// would let vary nonlinearly. Returns the new `Spring` kind only on the frame
/// something committed.
fn spring_section(
    ui: &mut egui::Ui,
    rest_length: f32,
    stiffness: f32,
    damping: f32,
    range: Option<[f32; 2]>,
) -> Option<JointKind> {
    let mut rest = rest_length;
    let mut k = stiffness;
    let mut c = damping;
    let mut new_range = range;
    let mut edited = false;

    ui.horizontal(|ui| {
        ui.label("rest length");
        if let Commit::Done(..) = precise_drag(
            ui,
            egui::Id::new("spring-rest"),
            &mut rest,
            rest_length,
            1.0,
        ) {
            edited = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("stiffness");
        if let Commit::Done(..) = precise_drag(
            ui,
            egui::Id::new("spring-k"),
            &mut k,
            DEFAULT_SPRING_STIFFNESS,
            50.0,
        ) {
            edited = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("damping");
        if let Commit::Done(..) = precise_drag(
            ui,
            egui::Id::new("spring-c"),
            &mut c,
            DEFAULT_SPRING_DAMPING,
            0.5,
        ) {
            edited = true;
        }
    });

    // Optional hard length clamp; off by default (unbounded travel).
    let mut clamped = new_range.is_some();
    if ui.checkbox(&mut clamped, "clamp length range").changed() {
        new_range = clamped.then(|| [0.0, (rest_length * 2.0).max(10.0)]);
        edited = true;
    }
    if let Some(bounds) = new_range.as_mut() {
        ui.horizontal(|ui| {
            ui.label("min");
            if let Commit::Done(..) =
                precise_drag(ui, egui::Id::new("spring-lo"), &mut bounds[0], 0.0, 1.0)
            {
                edited = true;
            }
            ui.label("max");
            if let Commit::Done(..) = precise_drag(
                ui,
                egui::Id::new("spring-hi"),
                &mut bounds[1],
                rest_length,
                1.0,
            ) {
                edited = true;
            }
        });
        if bounds[0] > bounds[1] {
            bounds.swap(0, 1);
        }
    }

    if !edited {
        return None;
    }
    Some(JointKind::Spring {
        rest_length: rest.max(0.0),
        stiffness: k.max(0.0),
        damping: c.max(0.0),
        range: new_range,
    })
}
