use crate::prelude::*;

pub struct ConstraintsPlugin;

impl Plugin for ConstraintsPlugin {
    fn build(&self, _app: &mut App) {
        // Register custom constraints here
        // app.add_systems(SubstepSolverSet::SolveUserConstraints, solve_gears);
    }
}

// Placeholder for GearJoint
// Spec: C = theta1 - theta1_init + r * (theta2 - theta2_init)
// See SPEC.md Section 3.2
#[derive(Component)]
pub struct GearJoint {
    pub entity_a: Entity,
    pub entity_b: Entity,
    pub ratio: f64,
}

// Placeholder for PulleyJoint
// Spec: |x1 - a1| + |x2 - a2| <= L
// See SPEC.md Section 3.3
#[derive(Component)]
pub struct PulleyJoint {
    pub entity_a: Entity,
    pub entity_b: Entity,
    pub anchor_a: Vec2,
    pub anchor_b: Vec2,
    pub length: f64,
}
