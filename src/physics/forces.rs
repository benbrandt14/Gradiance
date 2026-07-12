//! Field forces: the seam that turns authored field sources into solver
//! forces. First occupant: magnetism (M20) over the **SDF substrate** —
//! the force between two magnetized bodies follows the *surface* of the
//! other body (SDF distance + gradient), not its center, so a long plank
//! magnet pulls toward its nearest face like a real field.
//!
//! Forces are one-shot torques/forces through avian's `Forces` query data
//! (cleared each step), so constraints always win and nothing here is
//! authored, undoable, or persisted.

use crate::core::constants::PIXELS_PER_METER;
use crate::domain::Body;
use crate::domain::magnet::Magnet;
use crate::domain::shape::ShapeDef;
use crate::geometry::sdf;
use avian2d::prelude::*;
use bevy::prelude::*;

/// Force magnitude clamp (keeps close-contact fields from exploding).
const MAX_MAGNET_FORCE: f32 = 5.0e7;
/// Below this magnitude a pair is skipped (range cutoff without a knob).
const MIN_MAGNET_FORCE: f32 = 1.0;
/// Finite-difference step for the SDF gradient (world px).
const GRADIENT_EPS: f32 = 0.5;

/// Applies pairwise magnet forces between all magnetized dynamic bodies.
///
/// For each pair, the force on A points along the SDF gradient of B's
/// shape at A's center (i.e. toward/away from B's nearest surface), with
/// magnitude `sa·sb / (1 + d/m)^falloff` where `d` is A's surface distance
/// to B and `m` one meter. Equal and opposite on B. Positive
/// `strength_a * strength_b` attracts, negative repels.
pub fn apply_magnet_forces(
    magnets: Query<(Entity, &Magnet, &ShapeDef, &Transform), With<Body>>,
    mut forces: Query<Forces>,
) {
    let sources: Vec<(Entity, Magnet, &ShapeDef, Vec2, f32)> = magnets
        .iter()
        .map(|(e, m, shape, t)| {
            let pose = crate::core::units::PosRot::from_transform(t);
            (e, *m, shape, pose.pos, pose.rot)
        })
        .collect();

    for i in 0..sources.len() {
        for j in (i + 1)..sources.len() {
            let (ea, ma, _, pa, _) = sources[i];
            let (eb, mb, shape_b, pb, rot_b) = sources[j];
            // A's center in B's local frame: SDF distance + gradient give
            // the nearest-surface direction on B.
            let local_a = Vec2::from_angle(-rot_b).rotate(pa - pb);
            let d = sdf::eval(shape_b, local_a).max(0.0);
            let grad_local = sdf::gradient(shape_b, local_a, GRADIENT_EPS);
            let grad_world = Vec2::from_angle(rot_b).rotate(grad_local);
            let away = grad_world.normalize_or_zero();
            if away == Vec2::ZERO {
                continue;
            }

            let coupling = ma.strength * mb.strength;
            let falloff = 0.5 * (ma.falloff + mb.falloff);
            let magnitude =
                (coupling.abs() / (1.0 + d / PIXELS_PER_METER).powf(falloff)).min(MAX_MAGNET_FORCE);
            if magnitude < MIN_MAGNET_FORCE {
                continue;
            }
            // Positive coupling attracts: pull A toward B's surface
            // (against the outward gradient).
            let force_on_a = away * -coupling.signum() * magnitude;

            if let Ok(mut f) = forces.get_mut(ea) {
                f.apply_force(force_on_a);
            }
            if let Ok(mut f) = forces.get_mut(eb) {
                f.apply_force(-force_on_a);
            }
        }
    }
}
