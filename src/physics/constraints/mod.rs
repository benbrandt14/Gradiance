//! Custom constraints like Gears, Pulleys, and Linkages.

use crate::prelude::*;

/// Plugin for registering custom physics constraints.
pub struct ConstraintsPlugin;

impl Plugin for ConstraintsPlugin {
    fn build(&self, _app: &mut App) {
        // Register custom constraints here
        // app.add_systems(SubstepSolverSet::SolveUserConstraints, solve_gears);
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
