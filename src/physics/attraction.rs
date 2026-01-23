//! Attraction/Repulsion physics.
//!
//! Provides components and systems for applying gravitational-like forces between entities.

use crate::prelude::*;

/// A component that makes an entity attract or repel other entities.
#[derive(Component, Reflect, Debug, Clone, Copy)]
#[reflect(Component)]
pub struct Attractor {
    /// The strength of the attraction force.
    /// Positive values attract, negative values repel.
    pub strength: f32,
    /// The range of influence.
    pub range: f32,
}

impl Default for Attractor {
    fn default() -> Self {
        Self {
            strength: 0.0,
            range: 500.0,
        }
    }
}


/// Plugin that handles attraction physics.
pub struct AttractionPlugin;

impl Plugin for AttractionPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Attractor>();
        // Apply forces in FixedUpdate, before the physics step (which usually runs in PostUpdate or at the end of FixedUpdate)
        app.add_systems(FixedUpdate, apply_attraction_forces);
    }
}

/// System to apply attraction/repulsion forces.
fn apply_attraction_forces(
    attractors: Query<(Entity, &GlobalTransform, &Attractor)>,
    mut targets: Query<(
        Entity,
        &GlobalTransform,
        Option<&ReadMassProperties>,
        &mut ExternalForce,
    )>,
) {
    // Clear forces on all targets first to prevent accumulation
    for (_, _, _, mut external_force) in targets.iter_mut() {
        external_force.force = Vec2::ZERO;
        external_force.torque = 0.0;
    }

    for (attractor_entity, attractor_transform, attractor) in attractors.iter() {
        if attractor.strength.abs() < f32::EPSILON {
            continue;
        }

        let attractor_pos = attractor_transform.translation().truncate();

        for (target_entity, target_transform, mass_props_opt, mut external_force) in
            targets.iter_mut()
        {
            if attractor_entity == target_entity {
                continue;
            }

            let target_pos = target_transform.translation().truncate();
            let delta = attractor_pos - target_pos;
            let distance_sq = delta.length_squared();

            // Check range
            if distance_sq > attractor.range * attractor.range {
                continue;
            }

            // Calculate force
            // Formula: F = strength * mass / (dist^2 + epsilon)
            // Direction is normalized delta.
            let mass = mass_props_opt.map(|m| m.mass).unwrap_or(1.0);

            // Softening parameter to prevent infinity and extremely high forces at very close range
            let epsilon = 100.0;
            let force_magnitude = (attractor.strength * mass) / (distance_sq + epsilon);

            let force_dir = if distance_sq > 0.0001 {
                delta.normalize()
            } else {
                Vec2::ZERO
            };

            let force = force_dir * force_magnitude;

            external_force.force += force;
        }
    }
}
