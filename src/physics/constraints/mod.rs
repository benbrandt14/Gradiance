//! Custom constraints like Gears, Pulleys, and Linkages.

use crate::prelude::*;

/// Plugin for registering custom physics constraints.
pub struct ConstraintsPlugin;

impl Plugin for ConstraintsPlugin {
    fn build(&self, app: &mut App) {
        // Register custom constraints
        // Run before physics step to apply forces/impulses
        app.add_systems(FixedUpdate, (solve_gears, solve_pulleys).before(PhysicsSet::StepSimulation));
    }
}

/// A Gear Joint constraining the angular velocity of two bodies.
///
/// **Equation:** `ΔθA + rΔθB = 0`
#[derive(Component, Debug, Clone, Copy)]
pub struct GearJoint {
    /// The first body in the gear pair.
    pub entity_a: Entity,
    /// The second body in the gear pair.
    pub entity_b: Entity,
    /// The gear ratio (r).
    pub ratio: f32,
}

/// A Pulley Joint constraining the combined distance of two bodies from their anchors.
///
/// **Equation:** `|x1 - a1| + |x2 - a2| <= L`
#[derive(Component, Debug, Clone, Copy)]
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
    pub length: f32,
}

fn solve_gears(
    query: Query<&GearJoint>,
    mut bodies: Query<(&mut Velocity, &ReadMassProperties)>,
) {
    for gear in query.iter() {
        if let Ok([mut b1, mut b2]) = bodies.get_many_mut([gear.entity_a, gear.entity_b]) {
            let (mut v1, m1) = b1;
            let (mut v2, m2) = b2;

            let w1 = v1.angvel;
            let w2 = v2.angvel;
            let r = gear.ratio;

            let c_dot = w1 + r * w2;

            // Inv inertia
            let inv_i1 = m1.0.inv_principal_inertia_sqrt.powi(2);
            let inv_i2 = m2.0.inv_principal_inertia_sqrt.powi(2);

            let denom = inv_i1 + r * r * inv_i2;
            if denom.abs() < 1e-6 {
                continue;
            }

            let impulse = -c_dot / denom;

            v1.angvel += impulse * inv_i1;
            v2.angvel += impulse * r * inv_i2;
        }
    }
}

fn solve_pulleys(
    query: Query<&PulleyJoint>,
    mut bodies: Query<(&Transform, &mut ExternalForce)>,
) {
    for pulley in query.iter() {
        if let Ok([mut b1, mut b2]) = bodies.get_many_mut([pulley.entity_a, pulley.entity_b]) {
            let (t1, mut f1) = b1;
            let (t2, mut f2) = b2;

            let p1 = t1.translation.truncate();
            let p2 = t2.translation.truncate();

            let d1 = p1 - pulley.anchor_a;
            let d2 = p2 - pulley.anchor_b;

            let l1 = d1.length();
            let l2 = d2.length();

            let current_len = l1 + l2;

            if current_len > pulley.length {
                // Apply spring-like correction force (soft constraint)
                let err = current_len - pulley.length;
                let k = 50000.0; // Stiff spring constant

                if l1 > 0.001 {
                    let dir1 = d1 / l1;
                    f1.force += -dir1 * k * err;
                }
                if l2 > 0.001 {
                    let dir2 = d2 / l2;
                    f2.force += -dir2 * k * err;
                }
            }
        }
    }
}
