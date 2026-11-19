//! Oscillating motors: flip the native motor's target velocity at the
//! joint limits.
//!
//! Everything else about motors is handled natively by avian
//! (velocity tracking, max force, damping); this system only implements
//! the Algodoo "back and forth" behavior on top.

use crate::drive::SignalDrive;
use avian2d::prelude::{PrismaticJoint, RevoluteJoint};
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_core::units::PosRot;
use gradiance_domain::joint::{JointDef, JointKind};

/// Angular/linear buffer before a limit at which the motor reverses.
const ANGLE_BUFFER: f32 = 0.05;
const TRANSLATION_BUFFER: f32 = 0.02;

/// Reverses oscillating motors at their limits.
pub fn drive_oscillating_motors(
    index: Res<IdIndex>,
    defs: Query<(Entity, &JointDef, Option<&SignalDrive>)>,
    transforms: Query<&GlobalTransform>,
    mut revolutes: Query<&mut RevoluteJoint>,
    mut prismatics: Query<&mut PrismaticJoint>,
) {
    for (entity, def, drive) in &defs {
        // A signal-driven speed wins over the oscillation (brief N-1): the
        // signal is the person's chosen direction.
        if drive.is_some_and(|d| d.motor_speed.is_some()) {
            continue;
        }
        let Some(a) = index.entity(def.body_a) else {
            continue;
        };
        let rot = |e: Entity| {
            transforms.get(e).map_or(0.0, |t| {
                t.to_scale_rotation_translation()
                    .1
                    .to_euler(EulerRot::ZYX)
                    .0
            })
        };
        let pos = |e: Entity| {
            transforms
                .get(e)
                .map_or_default(|t| t.translation().truncate())
        };

        match &def.kind {
            JointKind::Hinge {
                limits: Some([min, max]),
                motor: Some(m),
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = revolutes.get_mut(entity) else {
                    continue;
                };
                let rot_b = def.body_b.and_then(|id| index.entity(id)).map_or(0.0, rot);
                // Reverse in avian's constraint frame (rest basis included, so
                // the reversal lines up with the real limit — the old code
                // omitted the basis and the motor just drove into the stop).
                // Both the angle and the reversal decision are pure, tested
                // domain functions.
                let rel = def.hinge_angle(rot(a), rot_b);
                if let Some(v) = gradiance_domain::joint::oscillate_target(
                    rel,
                    min.value(),
                    max.value(),
                    m.target_velocity.value(),
                    ANGLE_BUFFER,
                ) {
                    joint.motor.target_velocity = v;
                }
            }
            JointKind::Slider {
                limits: Some([min, max]),
                motor: Some(m),
                ..
            } if m.oscillate && m.enabled => {
                let Ok(mut joint) = prismatics.get_mut(entity) else {
                    continue;
                };
                // The travel in the engine joint's frame, world pins included
                // (`JointDef::slider_travel`; a world pin's body B is the
                // world at the identity pose).
                let pose = |e: Entity| PosRot {
                    pos: pos(e),
                    rot: rot(e),
                };
                let pose_b = match def.body_b {
                    Some(id) => {
                        let Some(b) = index.entity(id) else { continue };
                        pose(b)
                    }
                    None => gradiance_domain::joint::WORLD_POSE,
                };
                let Some(t) = def.slider_travel(pose(a), pose_b) else {
                    continue;
                };
                if let Some(v) = gradiance_domain::joint::oscillate_target(
                    t,
                    min.value(),
                    max.value(),
                    m.target_velocity.value(),
                    TRANSLATION_BUFFER,
                ) {
                    joint.motor.target_velocity = v;
                }
            }
            _ => {}
        }
    }
}
