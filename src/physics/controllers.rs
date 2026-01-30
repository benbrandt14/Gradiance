//! Logic for controlling joint motors (oscillation, etc).

use crate::prelude::*;

/// Component defining the behavior of a joint motor.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub enum MotorController {
    /// Simple oscillation between limits.
    Oscillator {
        /// Tolerance distance/angle from limit to trigger reversal.
        threshold: f32,
    },
    // Future: Tweening, PID, etc.
}

impl Default for MotorController {
    fn default() -> Self {
        Self::Oscillator { threshold: 0.05 }
    }
}

/// Plugin for motor controllers.
pub struct MotorControllerPlugin;

impl Plugin for MotorControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_motor_controllers);
    }
}

fn update_motor_controllers(
    mut joints: Query<(&mut ImpulseJoint, &RapierImpulseJointHandle, Option<&MotorController>)>,
    rapier_context_query: Query<&RapierContext>,
) {
    let Some(context) = rapier_context_query.iter().next() else {
        return;
    };

    for (mut joint, handle, controller) in &mut joints {
        // Only process if a controller is present OR if we want default behavior for limits?
        // User requested: "default hinge behavior should not oscillate... (and only oscillate when it's bounds are set)"
        // Since we don't attach MotorController by default, we can use its presence to opt-in.
        // OR we can infer "Oscillate if limits exist" as the default legacy behavior if no component is there,
        // BUT the user specifically asked for "only oscillate when bounds are set".
        // My previous logic checked `if let Some(limits) = ...`.
        // To support "rotate forever" vs "oscillate", we need to distinguish.
        // However, "Rotate Forever" implies no limits.
        // If limits ARE set, what else can it do? Saturate (stop) or Oscillate (bounce).
        // Standard physics engine behavior is Saturate.
        // User wants Oscillate when bounds are set.
        // So, if bounds are set, we oscillate. If not, we rotate.
        // This matches my previous logic: `if let Some(limits)`.
        // So we don't strictly *need* a component for the default behavior requested,
        // but for extensibility we should structure it nicely.

        // Let's stick to the previous logic but inside this cleaner system,
        // and maybe implicitly assume "Oscillator" behavior if limits are set,
        // until we add a specific component to override it (e.g. "SaturateAtLimit").

        if let Some(rapier_joint) = context.impulse_joints.get(handle.0) {
            match &mut joint.data {
                TypedJoint::RevoluteJoint(rev) => {
                    if let Some(limits) = rev.limits() {
                        // Default threshold
                        let threshold = if let Some(MotorController::Oscillator { threshold }) = controller {
                            *threshold
                        } else {
                            0.05 // Default implicit behavior
                        };

                        let mut current_angle = 0.0;
                        if let (Some(b1), Some(b2)) = (context.bodies.get(rapier_joint.body1), context.bodies.get(rapier_joint.body2)) {
                             current_angle = rapier_joint.data.as_revolute().unwrap().angle(b1.rotation(), b2.rotation());
                        }

                        if let Some(raw_component) = rev.data.raw.as_revolute() {
                            let motor = &raw_component.data.motors[2];
                            let target_vel = motor.target_vel;
                            let max_force = motor.max_force;
                            let damping = motor.damping;

                            if max_force > 0.0 && target_vel != 0.0 {
                                let min = limits.min;
                                let max = limits.max;

                                let mut new_vel = target_vel;

                                if current_angle <= min + threshold && target_vel < 0.0 {
                                    new_vel = -target_vel;
                                } else if current_angle >= max - threshold && target_vel > 0.0 {
                                    new_vel = -target_vel;
                                }

                                if (new_vel - target_vel).abs() > 1e-5 {
                                    rev.set_motor_velocity(new_vel, damping);
                                }
                            }
                        }
                    }
                }
                TypedJoint::PrismaticJoint(prism) => {
                    if let Some(limits) = prism.limits() {
                        let threshold = if let Some(MotorController::Oscillator { threshold }) = controller {
                            *threshold
                        } else {
                            0.1
                        };

                        let mut current_pos = 0.0;
                        if let (Some(b1), Some(b2)) = (context.bodies.get(rapier_joint.body1), context.bodies.get(rapier_joint.body2)) {
                             let anchor1 = prism.data.local_anchor1();
                             let anchor2 = prism.data.local_anchor2();
                             let axis1 = prism.data.local_axis1();

                             let t1 = b1.position();
                             let t2 = b2.position();

                             let p1 = t1.transform_point(&bevy_rapier2d::rapier::math::Point::from([anchor1.x, anchor1.y]));
                             let p2 = t2.transform_point(&bevy_rapier2d::rapier::math::Point::from([anchor2.x, anchor2.y]));
                             let axis_world = t1.rotation * bevy_rapier2d::rapier::math::Vector::from([axis1.x, axis1.y]);

                             let d = p2 - p1;
                             current_pos = d.dot(&axis_world);
                        }

                        if let Some(raw_component) = prism.data.raw.as_prismatic() {
                            let motor = &raw_component.data.motors[0];
                            let target_vel = motor.target_vel;
                            let max_force = motor.max_force;
                            let damping = motor.damping;

                            if max_force > 0.0 && target_vel != 0.0 {
                                let min = limits.min;
                                let max = limits.max;

                                let mut new_vel = target_vel;

                                if current_pos <= min + threshold && target_vel < 0.0 {
                                    new_vel = -target_vel;
                                } else if current_pos >= max - threshold && target_vel > 0.0 {
                                    new_vel = -target_vel;
                                }

                                if (new_vel - target_vel).abs() > 1e-5 {
                                    prism.set_motor_velocity(new_vel, damping);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
