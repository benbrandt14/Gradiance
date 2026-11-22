//! Draws particle water (brief S-10 spike) as discs: one overlay circle
//! per particle, half a spacing across. A surface render is a later brief.

use bevy::prelude::*;
use gradiance_interaction::overlay::OverlayGizmos;
use gradiance_physics::particles::ParticleFluid;

/// The water's disc colour.
const WATER: Color = Color::srgba(0.25, 0.55, 0.95, 0.85);

/// Draws every particle.
pub fn draw_particles(fluid: Res<ParticleFluid>, mut gizmos: Gizmos<OverlayGizmos>) {
    let radius = 0.5 * fluid.params.spacing;
    for p in &fluid.particles.pos {
        gizmos.circle_2d(Isometry2d::from_translation(*p), radius, WATER);
    }
}
