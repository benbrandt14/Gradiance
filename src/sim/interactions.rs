//! Particle ↔ scene interactions — the extensible seam.
//!
//! Each interaction is an **independent, additive system** over the `SoA`
//! population plus *read* facades (`physics::queries`, `physics::fields`),
//! mirroring the fields substrate's "one cut-point, many consumers" shape
//! (`docs/field-architecture.md`). Reads are total; the only writes are to
//! the derived particle buffer and — for two-way coupling — the sanctioned
//! `Forces` seam. Adding a new interaction (absorb, stick, charge) is a new
//! sibling system + a per-group toggle; nothing here is a new mutation path.
//!
//! First occupant: **rigid collision**, gated on depth-band overlap so a
//! group obeys collision layers *as one entity* — a cloud on a
//! non-overlapping layer passes straight through a body.

use crate::domain::Body;
use crate::domain::depth::DepthBand;
use crate::physics::queries::PhysicsQueries;
use crate::sim::bridge::{Particles, SimConfig};
use crate::sim::groups::ParticleGroups;
use bevy::prelude::*;

/// Small outward offset so a resolved particle sits *just* outside the
/// surface, not exactly on it (avoids immediate re-penetration).
const SKIN: f32 = 0.5;

/// Reflects a velocity about an outward unit normal with restitution,
/// but only when it is moving *into* the surface. Pure — unit-tested.
pub fn reflect_velocity(v: Vec2, normal: Vec2, restitution: f32) -> Vec2 {
    let vn = v.dot(normal);
    if vn >= 0.0 {
        return v; // already separating; leave it
    }
    v - (1.0 + restitution.clamp(0.0, 1.0)) * vn * normal
}

/// Pushes penetrating particles out of rigid bodies and reflects their
/// velocity — the one-way half of rigid coupling (the body's reaction is
/// S4c). Gated on `DepthBand::overlaps`: a particle whose group band does
/// not overlap the struck body's band passes through (collision layers).
pub fn collide_with_bodies(
    physics: PhysicsQueries,
    depths: Query<&DepthBand, With<Body>>,
    groups: Res<ParticleGroups>,
    config: Res<SimConfig>,
    mut particles: ResMut<Particles>,
) {
    let restitution = config.restitution;
    let state = &mut particles.0;
    for i in 0..state.len() {
        let p = state.pos[i];
        let Some(hit) = physics.project_point(p) else {
            continue;
        };
        if !hit.is_inside {
            continue; // not penetrating
        }
        // Collision-layer gate: the group's band vs the body's band.
        let Ok(body_band) = depths.get(hit.entity) else {
            continue;
        };
        if !groups.get(state.group[i]).depth.overlaps(body_band) {
            continue; // different layers — pass through
        }
        // Outward normal: from the (interior) particle toward its nearest
        // surface point. Push the particle just outside, reflect velocity.
        let normal = (hit.point - p).normalize_or_zero();
        if normal == Vec2::ZERO {
            continue;
        }
        state.pos[i] = hit.point + normal * SKIN;
        state.vel[i] = reflect_velocity(state.vel[i], normal, restitution);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflect_bounces_only_incoming_velocity() {
        let n = Vec2::Y; // surface faces up
        // Moving down into the surface, elastic → flips to up.
        assert_eq!(
            reflect_velocity(Vec2::new(0.0, -10.0), n, 1.0),
            Vec2::new(0.0, 10.0)
        );
        // Inelastic (e=0) → tangential kept, normal zeroed.
        assert_eq!(
            reflect_velocity(Vec2::new(3.0, -10.0), n, 0.0),
            Vec2::new(3.0, 0.0)
        );
        // Already separating (moving up) → unchanged.
        assert_eq!(
            reflect_velocity(Vec2::new(0.0, 5.0), n, 0.5),
            Vec2::new(0.0, 5.0)
        );
    }
}
