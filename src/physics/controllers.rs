//! Physics controllers for motors and other dynamic behaviors.

use crate::prelude::*;
use bevy_rapier2d::prelude::*;
use bevy_rapier2d::rapier::dynamics::{JointAxesMask, JointAxis};

/// Component that controls a joint motor to oscillate between limits.
#[derive(Component, Debug, Clone, Copy)]
pub struct MotorController {
    /// Target velocity of the motor.
    pub target_vel: f32,
    /// Damping factor.
    pub damping: f32,
    /// Maximum force.
    pub max_force: f32,
    /// Whether to oscillate between limits.
    pub oscillate: bool,
}

impl Default for MotorController {
    fn default() -> Self {
        Self {
            target_vel: 1.0,
            damping: 0.0,
            max_force: 1000.0,
            oscillate: true,
        }
    }
}

/// System to update motor controllers.
pub fn motor_controller_system(
    mut query: Query<(Entity, &mut MotorController, &mut ImpulseJoint)>,
    transforms: Query<&GlobalTransform>,
) {
    for (entity, mut controller, mut joint) in &mut query {
        if !controller.oscillate {
            continue;
        }

        // Helper to get angle
        let get_angle = |e_a,
                         e_b,
                         _local_anchor1: Vec2,
                         _local_anchor2: Vec2,
                         transforms: &Query<&GlobalTransform>| {
            if let Ok(t_a) = transforms.get(e_a)
                && let Ok(t_b) = transforms.get(e_b)
            {
                // The angle is relative rotation, local anchors don't affect rotation difference,
                // but we should compute it consistently.
                let rot_a = t_a.compute_transform().rotation.to_euler(EulerRot::XYZ).2;
                let rot_b = t_b.compute_transform().rotation.to_euler(EulerRot::XYZ).2;

                // Keep it in [-PI, PI] range
                let mut diff = rot_a - rot_b;
                while diff > std::f32::consts::PI {
                    diff -= std::f32::consts::TAU;
                }
                while diff < -std::f32::consts::PI {
                    diff += std::f32::consts::TAU;
                }
                diff
            } else {
                0.0
            }
        };

        let get_dist = |e_a,
                        e_b,
                        local_anchor1: Vec2,
                        local_anchor2: Vec2,
                        transforms: &Query<&GlobalTransform>| {
            if let Ok(t_a) = transforms.get(e_a)
                && let Ok(t_b) = transforms.get(e_b)
            {
                let world_anchor1 = t_a
                    .compute_transform()
                    .transform_point(local_anchor1.extend(0.0));
                let world_anchor2 = t_b
                    .compute_transform()
                    .transform_point(local_anchor2.extend(0.0));

                let diff = world_anchor1 - world_anchor2;
                diff.length()
            } else {
                0.0
            }
        };

        let (position, min_limit, max_limit) = match &joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                let (min, max) = if let Some(l) = rev.limits() {
                    (l.min, l.max)
                } else {
                    (-f32::MAX, f32::MAX)
                };
                (
                    get_angle(
                        entity,
                        joint.parent,
                        rev.local_anchor1(),
                        rev.local_anchor2(),
                        &transforms,
                    ),
                    min,
                    max,
                )
            }
            TypedJoint::PrismaticJoint(prism) => {
                let (min, max) = if let Some(l) = prism.limits() {
                    (l.min, l.max)
                } else {
                    (-f32::MAX, f32::MAX)
                };
                (
                    get_dist(
                        entity,
                        joint.parent,
                        prism.local_anchor1(),
                        prism.local_anchor2(),
                        &transforms,
                    ),
                    min,
                    max,
                )
            }
            TypedJoint::GenericJoint(g) => {
                // Check if Revolute (AngZ) or Prismatic (X)
                let locked = g.locked_axes();
                let x = locked.contains(JointAxesMask::LIN_X);
                let y = locked.contains(JointAxesMask::LIN_Y);
                let ang_z = locked.contains(JointAxesMask::ANG_X);

                if x && y && !ang_z {
                    // Revolute
                    let (min, max) = if let Some(l) = g.limits(JointAxis::AngX) {
                        (l.min, l.max)
                    } else {
                        (-f32::MAX, f32::MAX)
                    };
                    (
                        get_angle(
                            entity,
                            joint.parent,
                            g.local_anchor1(),
                            g.local_anchor2(),
                            &transforms,
                        ),
                        min,
                        max,
                    )
                } else if !x && y && ang_z {
                    // Prismatic
                    let (min, max) = if let Some(l) = g.limits(JointAxis::LinX) {
                        (l.min, l.max)
                    } else {
                        (-f32::MAX, f32::MAX)
                    };
                    (
                        get_dist(
                            entity,
                            joint.parent,
                            g.local_anchor1(),
                            g.local_anchor2(),
                            &transforms,
                        ),
                        min,
                        max,
                    )
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Check limits
        // We add a small buffer or hysteresis
        let buffer = 0.05;

        // For Hinge: angle is periodic? No, limits imply non-periodic.
        // If angle is wrapped, we might have issues. Rapier handles limits on unwrapped angle.
        // Our calc `rot_b - rot_a` is wrapped to [-PI, PI] usually?
        // `to_euler` returns [-PI, PI].
        // If limits are outside this range, this simple calculation fails.
        // But for now, let's assume limits are within range.

        if controller.target_vel > 0.0 && position >= max_limit - buffer {
            controller.target_vel = -controller.target_vel.abs();
        } else if controller.target_vel < 0.0 && position <= min_limit + buffer {
            controller.target_vel = controller.target_vel.abs();
        }

        // Apply to joint
        match &mut joint.data {
            TypedJoint::RevoluteJoint(rev) => {
                rev.set_motor_velocity(controller.target_vel, controller.damping);
                rev.set_motor_max_force(controller.max_force);
            }
            TypedJoint::PrismaticJoint(prism) => {
                prism.set_motor_velocity(controller.target_vel, controller.damping);
                prism.set_motor_max_force(controller.max_force);
            }
            TypedJoint::GenericJoint(g) => {
                let locked = g.locked_axes();
                let x = locked.contains(JointAxesMask::LIN_X);
                let y = locked.contains(JointAxesMask::LIN_Y);
                let ang_z = locked.contains(JointAxesMask::ANG_X);

                if x && y && !ang_z {
                    // Revolute
                    g.set_motor_velocity(
                        JointAxis::AngX,
                        controller.target_vel,
                        controller.damping,
                    );
                    g.set_motor_max_force(JointAxis::AngX, controller.max_force);
                } else if !x && y && ang_z {
                    // Prismatic
                    g.set_motor_velocity(
                        JointAxis::LinX,
                        controller.target_vel,
                        controller.damping,
                    );
                    g.set_motor_max_force(JointAxis::LinX, controller.max_force);
                }
            }
            _ => {}
        }
    }
}
