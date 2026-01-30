//! Custom constraints like Gears, Pulleys, and Linkages.
//!
//! Also handles motor control logic (oscillation).

use crate::prelude::*;

/// Plugin for registering custom physics constraints.
pub struct ConstraintsPlugin;

impl Plugin for ConstraintsPlugin {
    fn build(&self, app: &mut App) {
        // Register custom constraints here
        // app.add_systems(SubstepSolverSet::SolveUserConstraints, solve_gears);
        app.add_systems(Update, oscillate_motors);
    }
}

/// A Gear Joint constraining the angular velocity of two bodies.
///
/// **Equation:** `ΔθA + rΔθB = 0`
#[derive(Component)]
pub struct GearJoint {
    /// The first body in the gear pair.
    pub entity_a: Entity,
    /// The second body in the gear pair.
    pub entity_b: Entity,
    /// The gear ratio (r).
    pub ratio: f64,
}

/// A Pulley Joint constraining the combined distance of two bodies from their anchors.
///
/// **Equation:** `|x1 - a1| + |x2 - a2| <= L`
#[derive(Component)]
pub struct PulleyJoint {
    /// The first body.
    pub entity_a: Entity,
    /// The second body.
    pub entity_b: Entity,
    /// The anchor point for the first body.
    pub anchor_a: Vec2,
    /// The anchor point for the second body.
    pub anchor_b: Vec2,
    /// The maximum total length of the rope.
    pub length: f64,
}

/// System to oscillate motors when they hit their limits.
fn oscillate_motors(
    mut joints: Query<(&mut ImpulseJoint, &RapierImpulseJointHandle)>,
    rapier_context_query: Query<&RapierContext>,
) {
    let Some(context) = rapier_context_query.iter().next() else {
        return;
    };

    for (mut joint, handle) in &mut joints {
        if let Some(rapier_joint) = context.impulse_joints.get(handle.0) {
            match &mut joint.data {
                TypedJoint::RevoluteJoint(rev) => {
                    if let Some(limits) = rev.limits() {
                        let mut current_angle = 0.0;
                        if let (Some(b1), Some(b2)) = (context.bodies.get(rapier_joint.body1), context.bodies.get(rapier_joint.body2)) {
                             current_angle = rapier_joint.data.as_revolute().unwrap().angle(b1.rotation(), b2.rotation());
                        }

                        // Read motor settings from the component (Configuration source)
                        // Accessing via raw GenericJoint structure as seen in inspector.rs
                        if let Some(raw_component) = rev.data.raw.as_revolute() {
                            // JointAxis::AngZ = 2
                            let motor = &raw_component.data.motors[2];
                            let target_vel = motor.target_vel;
                            let max_force = motor.max_force;
                            let damping = motor.damping;

                            if max_force > 0.0 && target_vel != 0.0 {
                            let min = limits.min;
                            let max = limits.max;
                            let threshold = 0.05;

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
                        // Calculate current position manually since direct accessor might be missing or hard to find
                        let mut current_pos = 0.0;
                        if let (Some(b1), Some(b2)) = (context.bodies.get(rapier_joint.body1), context.bodies.get(rapier_joint.body2)) {
                             // Assuming rapier_joint structure matches what we expect from TypedJoint
                             // We use the component `prism` to get local anchors/axis because accessing them from `rapier_joint` (backend)
                             // might be tricky if we don't have the right imports or trait visibility.
                             // Actually, `prism` (component) has the config.
                             // `prism.data.local_anchor1()` etc.
                             // `TypedJoint::PrismaticJoint` wrapper in bevy_rapier exposes these methods on `PrismaticJoint` struct.

                             let anchor1 = prism.data.local_anchor1();
                             let anchor2 = prism.data.local_anchor2();
                             let axis1 = prism.data.local_axis1();

                             // Transform to world
                             let t1 = b1.position(); // Isometry
                             let t2 = b2.position();

                             // Calculate position: (t2*a2 - t1*a1) . (t1.rotation * axis1)
                             let p1 = t1.transform_point(&bevy_rapier2d::rapier::math::Point::from([anchor1.x, anchor1.y]));
                             let p2 = t2.transform_point(&bevy_rapier2d::rapier::math::Point::from([anchor2.x, anchor2.y]));
                             let axis_world = t1.rotation * bevy_rapier2d::rapier::math::Vector::from([axis1.x, axis1.y]);

                             let d = p2 - p1;
                             current_pos = d.dot(&axis_world);
                        }

                        // Read motor settings
                        if let Some(raw_component) = prism.data.raw.as_prismatic() {
                            // JointAxis::LinX = 0? Prismatic is usually 1 DOF along X.
                            let motor = &raw_component.data.motors[0];
                            let target_vel = motor.target_vel;
                            let max_force = motor.max_force;
                            let damping = motor.damping;

                            if max_force > 0.0 && target_vel != 0.0 {
                                let min = limits.min;
                                let max = limits.max;
                                let threshold = 0.1;

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
