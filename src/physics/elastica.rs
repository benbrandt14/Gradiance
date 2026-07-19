//! The flexure force system: authored [`JointKind::Elastica`] joints →
//! per-frame end wrenches via avian's `Forces` accumulator.
//!
//! This is the "constraint the solver never sees": the beam's restoring
//! loads come from the pure reduction in [`geometry::elastica`]
//! (see that module for the model) and enter the simulation as ordinary
//! accumulated forces, exactly like force fields (`physics::fields`) and
//! the grab spring. The per-rod warm-start cache lives in the derived
//! [`ElasticaCache`] component on the joint entity (inserted by
//! `joint_sync`, never persisted — a cold start re-converges within a
//! few frames).
//!
//! [`geometry::elastica`]: crate::geometry::elastica

use crate::core::ids::IdIndex;
use crate::domain::Body;
use crate::domain::joint::{JointDef, JointKind};
use crate::domain::rod::RodEndKind;
use crate::domain::settings::SimSettings;
use crate::geometry::elastica::{ElasticaInput, ElasticaParams, ElasticaState, solve};
use avian2d::prelude::*;
use bevy::prelude::*;

/// Derived warm-start cache for one elastica joint (never serialized;
/// rebuilt from zero on load).
#[derive(Component, Debug, Default)]
pub struct ElasticaCache(pub ElasticaState);

/// One end body's kinematic snapshot for the beam solve.
struct EndBody {
    pos: Vec2,
    rot: f32,
    /// Velocity of the anchor point (world).
    anchor_vel: Vec2,
    ang_vel: f32,
    inv_mass: f32,
    inv_inertia: f32,
    dynamic: bool,
}

/// Applies every flexure rod's end wrenches for this frame.
pub fn apply_elastica_forces(
    settings: Res<SimSettings>,
    index: Res<IdIndex>,
    mut joints: Query<(&JointDef, &mut ElasticaCache)>,
    mut bodies: Query<(Forces, &ComputedMass, &ComputedAngularInertia), With<Body>>,
    poses: Query<&Transform, With<Body>>,
) {
    // The accumulated force is held constant across the whole fixed step,
    // so the step (not the substep) is the explicit-stability timescale.
    let h_step = 1.0 / settings.timestep_hz.clamp(15.0, 240.0);

    for (def, mut cache) in &mut joints {
        let JointKind::Elastica {
            length,
            young_modulus,
            thickness,
            damping_ratio,
            end_a,
            end_b,
            tangent_a,
            tangent_b,
        } = def.kind
        else {
            continue;
        };
        // Elastica always couples two real bodies (the tool spawns tip
        // bodies for unattached ends); a world-pin def is skipped.
        let Some(id_b) = def.body_b else {
            continue;
        };
        let (Some(ent_a), Some(ent_b)) = (index.entity(def.body_a), index.entity(id_b)) else {
            continue;
        };
        if ent_a == ent_b {
            continue;
        }

        let anchor_world =
            |end: &EndBody, local: Vec2| end.pos + Vec2::from_angle(end.rot).rotate(local);
        let Some(a) = read_end(ent_a, def.anchor_a, &mut bodies, &poses) else {
            continue;
        };
        let Some(b) = read_end(ent_b, def.anchor_b, &mut bodies, &poses) else {
            continue;
        };
        if !a.dynamic && !b.dynamic {
            continue;
        }

        let pa = anchor_world(&a, def.anchor_a);
        let pb = anchor_world(&b, def.anchor_b);
        let d = pb - pa;
        let chord = d.length();
        if chord < 1.0e-3 || !chord.is_finite() {
            continue;
        }
        let x_hat = d / chord;
        let y_hat = x_hat.perp();
        let psi = x_hat.to_angle();

        let rel = b.anchor_vel - a.anchor_vel;
        let dc = rel.dot(x_hat);
        let v_t = rel.dot(y_hat);
        let dpsi = v_t / chord;

        let inv_m = a.inv_mass + b.inv_mass;
        let inv_i = a.inv_inertia + b.inv_inertia;
        let input = ElasticaInput {
            params: ElasticaParams {
                length: length.max(1.0),
                ea: young_modulus * thickness,
                ei: young_modulus * thickness.powi(3) / 12.0,
                hinge_a: end_a == RodEndKind::Hinge,
                hinge_b: end_b == RodEndKind::Hinge,
            },
            chord,
            phi_a: crate::geometry::wrap_angle(a.rot + tangent_a - psi),
            phi_b: crate::geometry::wrap_angle(b.rot + tangent_b - psi),
            dc,
            v_t,
            dphi_a: a.ang_vel - dpsi,
            dphi_b: b.ang_vel - dpsi,
            zeta: damping_ratio,
            m_red: if inv_m > 0.0 { 1.0 / inv_m } else { f32::MAX },
            i_red: if inv_i > 0.0 { 1.0 / inv_i } else { f32::MAX },
            h_step,
        };
        let loads = solve(&input, &mut cache.0);
        let f_world = Vec2::from_angle(psi).rotate(loads.force_b);
        if !(f_world.is_finite() && loads.moment_a.is_finite() && loads.moment_b.is_finite()) {
            cache.0 = ElasticaState::default();
            continue;
        }

        // Equal-and-opposite wrench pair at the two anchor points — zero
        // net force, zero net moment (see geometry::elastica).
        if b.dynamic
            && let Ok((mut forces, _, _)) = bodies.get_mut(ent_b)
        {
            forces.apply_force_at_point(f_world, pb);
            forces.apply_torque(loads.moment_b);
        }
        if a.dynamic
            && let Ok((mut forces, _, _)) = bodies.get_mut(ent_a)
        {
            forces.apply_force_at_point(-f_world, pa);
            forces.apply_torque(loads.moment_a);
        }
    }
}

/// Snapshots one end body's pose, anchor velocity, and inverse inertias.
/// Dynamic bodies read through `Forces`; static/kinematic ones fall back
/// to `Transform` with zero velocity and infinite inertia.
fn read_end(
    entity: Entity,
    anchor_local: Vec2,
    bodies: &mut Query<(Forces, &ComputedMass, &ComputedAngularInertia), With<Body>>,
    poses: &Query<&Transform, With<Body>>,
) -> Option<EndBody> {
    if let Ok((forces, mass, inertia)) = bodies.get_mut(entity) {
        let pos = Vec2::new(forces.position().x, forces.position().y);
        let rot = forces.rotation().as_radians();
        let anchor = pos + Vec2::from_angle(rot).rotate(anchor_local);
        let (m, i) = (mass.value(), inertia.value());
        return Some(EndBody {
            pos,
            rot,
            anchor_vel: forces.velocity_at_point(anchor),
            ang_vel: forces.angular_velocity(),
            inv_mass: if m.is_finite() && m > 0.0 {
                1.0 / m
            } else {
                0.0
            },
            inv_inertia: if i.is_finite() && i > 0.0 {
                1.0 / i
            } else {
                0.0
            },
            dynamic: true,
        });
    }
    let transform = poses.get(entity).ok()?;
    let pos = transform.translation.truncate();
    let rot = transform.rotation.to_euler(EulerRot::ZYX).0;
    Some(EndBody {
        pos,
        rot,
        anchor_vel: Vec2::ZERO,
        ang_vel: 0.0,
        inv_mass: 0.0,
        inv_inertia: 0.0,
        dynamic: false,
    })
}
