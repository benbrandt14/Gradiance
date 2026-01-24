//! Custom constraints like Gears, Pulleys, and Linkages.

use crate::prelude::*;

/// Plugin for registering custom physics constraints.
pub struct ConstraintsPlugin;

impl Plugin for ConstraintsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GearPhysicsSettings>();

        // Run solver before physics step to enforce kinematic relationships
        // NOTE: Rapier simulation step resets forces/impulses? No, impulses modify velocity directly.
        // We need to run AFTER the integration or BEFORE?
        // If we run before, Rapier's solver might override it if it handles collisions.
        // But for pure kinematics, it should be fine.
        // However, if we modify velocity, Rapier integrates it.
        // Let's ensure it runs.
        app.add_systems(
            FixedUpdate,
            solve_gears
                .before(PhysicsSet::StepSimulation),
        );
    }
}

/// Settings for analytic gear physics.
#[derive(Resource, Default)]
pub struct GearPhysicsSettings {
    /// Whether to enable analytic gear constraints (speed ratio enforcement).
    pub analytic_enabled: bool,
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

fn solve_gears(
    mut bodies: Query<(&mut Velocity, Option<&ReadMassProperties>, Option<&AdditionalMassProperties>)>,
    joints: Query<&GearJoint>,
    settings: Res<GearPhysicsSettings>,
) {
    if !settings.analytic_enabled {
        return;
    }

    for joint in joints.iter() {
        if let Ok([(mut vel_a, read_mass_a, add_mass_a), (mut vel_b, read_mass_b, add_mass_b)]) =
            bodies.get_many_mut([joint.entity_a, joint.entity_b])
        {
            let w_a = vel_a.angvel;
            let w_b = vel_b.angvel;
            let ratio = joint.ratio as f32;

            // Helper to get inertia
            let get_inv_inertia = |rm: Option<&ReadMassProperties>, am: Option<&AdditionalMassProperties>| -> f32 {
                if let Some(m) = rm {
                    if m.principal_inertia != 0.0 { return 1.0 / m.principal_inertia; }
                }
                if let Some(am) = am {
                    match am {
                        AdditionalMassProperties::MassProperties(props) => {
                            if props.principal_inertia != 0.0 { return 1.0 / props.principal_inertia; }
                        }
                        AdditionalMassProperties::Mass(_) => {
                            // Point mass? Assume some inertia based on mass?
                            // Without shape, we can't guess inertia from mass accurately.
                            // But typically Mass(m) implies Moment of Inertia I = m * r^2 (but r unknown)
                            // or for a circle I = 0.5 * m * r^2.
                            // If we can't find inertia, return 0.0 (static)
                            return 0.0;
                        }
                    }
                }
                0.0
            };

            // Constraint error: C = w_a + ratio * w_b
            // We want C = 0.
            let c = w_a + ratio * w_b;

            // Inverse inertia
            let inv_i_a = get_inv_inertia(read_mass_a, add_mass_a);
            let inv_i_b = get_inv_inertia(read_mass_b, add_mass_b);

            // Denominator: J M^-1 J^T
            // J = [1, ratio]
            // J M^-1 J^T = inv_i_a + ratio^2 * inv_i_b
            let k = inv_i_a + ratio * ratio * inv_i_b;

            if k < 1e-6 {
                continue; // Infinite inertia (static bodies), can't move them
            }

            // Impulse lambda
            // We correct the velocity fully in one step (Baumgarte stabilization for velocity level)
            let lambda = -c / k;

            // Apply impulses
            // dw_a = lambda * inv_i_a
            // dw_b = lambda * ratio * inv_i_b

            vel_a.angvel += lambda * inv_i_a;
            vel_b.angvel += lambda * ratio * inv_i_b;
        }
    }
}
