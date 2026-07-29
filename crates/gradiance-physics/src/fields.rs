//! The field seam: authored [`FieldSource`]s become solver forces, and one
//! **sampling cut-point** serves every field consumer.
//!
//! Architecture (`docs/field-architecture.md`):
//! - [`Fields::accel_at`] is the single read: it superposes every source's
//!   acceleration at a world point. The force applicator, the vector-plot
//!   overlay, [`SetOrbitRequest`], and future consumers (particle kernels,
//!   node-editor sensors, scripted probes) all sample through it, so a new
//!   field kind lands everywhere at once.
//! - Sources are **SDF-shaped by default**: magnitude decays over the
//!   source's surface distance, direction follows its SDF gradient.
//! - Fields **superpose** (sum of accelerations). Time variation arrives by
//!   driving `FieldSource::strength` through the existing seams (an
//!   authored edit, or a P2 signal driver) — the sampler stays pure.
//! - Forces are one-shot through [`ForceAccumulator`](crate::forces) (the
//!   solver's constraints always win); nothing here is authored, undoable, or
//!   persisted.
//! - Fields are **Newtonian**: every force gets an equal-and-opposite
//!   reaction on its source (momentum conserves), and a source's strength
//!   couples through its [`FieldMass`] (area × density), so cutting a source
//!   redistributes — never changes — its total field.

use crate::forces::ForceAccumulator;
use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::Body;
use gradiance_domain::field::{FieldFalloff, FieldSource};
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::sdf;
use gradiance_units::{
    Acceleration, Acceleration2, Density, Force2, Length, Mass, Torque, Velocity,
};

/// Acceleration magnitude clamp (keeps zero-distance fields sane).
const MAX_FIELD_ACCEL: Acceleration = Acceleration(500.0);
/// Below this magnitude a contribution is skipped (implicit range cutoff).
const MIN_FIELD_ACCEL: Acceleration = Acceleration(1.0e-4);
/// Finite-difference step for the SDF gradient (world metres).
const GRADIENT_EPS: f32 = 0.005;
/// The field mass at which a source's strength knob applies verbatim:
/// a 1 m² body at density 1 — i.e. `1.0` now that the world is SI (this was
/// `PIXELS_PER_METER²` when area was measured in px²). Bigger/denser sources
/// couple proportionally harder, smaller ones softer — so cutting a source
/// splits its pull instead of doubling it.
pub const REFERENCE_FIELD_MASS: Mass = Mass(1.0);

/// Derived field-coupling mass of a source: shape area × density. Rebuilt
/// by [`sync_field_mass`] on shape/density edits; never serialized, never
/// undo-recorded (`docs/architecture.md`). Kept separate from avian's
/// `ComputedMass` so static sources (a pinned "sun") still couple.
#[derive(Component, Debug, Clone, Copy)]
pub struct FieldMass(pub Mass);

/// Derives [`FieldMass`] for every field source whose shape or density
/// changed (cut pieces, resizes, density edits all land here).
pub fn sync_field_mass(
    mut commands: Commands,
    changed: Query<
        (Entity, &ShapeDef, &ColliderDensity),
        (
            With<FieldSource>,
            Or<(
                Added<FieldSource>,
                Changed<FieldSource>,
                Changed<ShapeDef>,
                Changed<ColliderDensity>,
            )>,
        ),
    >,
) {
    for (entity, shape, density) in &changed {
        let area = gradiance_units::Area(gradiance_geometry::polygonize::polygonize(shape).area());
        commands
            .entity(entity)
            .insert(FieldMass(gradiance_units::mass_of(
                Density(density.0),
                area,
            )));
    }
}

/// Falloff factor over surface distance `d` (metres) — pure, unit-testable.
pub fn falloff_factor(falloff: FieldFalloff, d: f32) -> f32 {
    let x = 1.0 + d.max(0.0);
    match falloff {
        FieldFalloff::Linear => 1.0 / x,
        FieldFalloff::Quadratic => 1.0 / (x * x),
    }
}

/// The circular-orbit speed for field acceleration `a` at radius `r` —
/// `v = √(a·r)`, the limit-cycle condition `a = v²/r`. Pure.
///
/// The radius is floored just above zero purely to keep a degenerate
/// zero-radius orbit defined. It used to be floored at `1.0` — a pixel-era
/// guard (1 px) the SI flip missed: at metre scale that silently substituted
/// `r = 1 m` for every orbit set inside a metre, overstating `v` so the body
/// flew off instead of circling.
pub fn orbit_speed(accel: Acceleration, radius: Length) -> Velocity {
    Velocity((accel.value().abs() * radius.value().max(f32::EPSILON)).sqrt())
}

/// **The sampling cut-point.** Superposed field acceleration at any world
/// point; every consumer of the field reads through this.
#[derive(SystemParam)]
pub struct Fields<'w, 's> {
    sources: Query<
        'w,
        's,
        (
            Entity,
            &'static FieldSource,
            &'static FieldMass,
            &'static ShapeDef,
            &'static Transform,
        ),
        With<Body>,
    >,
}

impl Fields<'_, '_> {
    /// Superposed field acceleration at `p`, excluding `exclude` (a body
    /// never accelerates itself with its own field).
    pub fn accel_at(&self, p: Vec2, exclude: Option<Entity>) -> Acceleration2 {
        Acceleration2::new(
            self.contributions(p, exclude)
                .map(|(_, accel)| accel.value())
                .sum(),
        )
    }

    /// The single strongest contributor at `p` (the "dominant attractor"
    /// set-in-orbit revolves around), with its acceleration contribution.
    pub fn dominant_at(&self, p: Vec2, exclude: Option<Entity>) -> Option<(Entity, Acceleration2)> {
        self.contributions(p, exclude).max_by(|(_, a), (_, b)| {
            a.value()
                .length_squared()
                .total_cmp(&b.value().length_squared())
        })
    }

    /// Per-source contributions at `p` (skips negligible ones). Public so
    /// the force applicator can mirror each contribution back onto its
    /// source (Newton's third law); other consumers want [`Self::accel_at`].
    pub fn contributions(
        &self,
        p: Vec2,
        exclude: Option<Entity>,
    ) -> impl Iterator<Item = (Entity, Acceleration2)> {
        self.sources
            .iter()
            .filter(move |(e, ..)| Some(*e) != exclude)
            .filter_map(move |(entity, source, field_mass, shape, transform)| {
                let pose = gradiance_core::units::PosRot::from_transform(transform);
                let local = Vec2::from_angle(-pose.rot).rotate(p - pose.pos);
                let d = sdf::eval(shape, local).max(0.0);
                let away = Vec2::from_angle(pose.rot)
                    .rotate(sdf::gradient(shape, local, GRADIENT_EPS))
                    .normalize_or_zero();
                if away == Vec2::ZERO {
                    return None;
                }
                // Positive strength repels: acceleration along the outward
                // gradient. Negative attracts. Coupling scales with the
                // source's field mass, so a cut redistributes the field
                // across the pieces instead of duplicating it.
                let coupling = source.strength * (field_mass.0 / REFERENCE_FIELD_MASS);
                let magnitude = (coupling * falloff_factor(source.falloff, d))
                    .clamp(-MAX_FIELD_ACCEL.value(), MAX_FIELD_ACCEL.value());
                let accel = Acceleration2::new(away * magnitude);
                (accel.magnitude() >= MIN_FIELD_ACCEL).then_some((entity, accel))
            })
    }
}

/// Applies field forces to every dynamic body (mass-scaled, so field
/// acceleration is mass-independent — gravity-like, and orbits work), and
/// mirrors every force back onto its source (equal and opposite, so an
/// attractor is pulled toward what it attracts and momentum conserves).
pub fn apply_field_forces(
    fields: Fields,
    targets: Query<(Entity, &RigidBody, &ComputedMass, &Transform), With<Body>>,
    mut accumulator: ResMut<ForceAccumulator>,
) {
    for (entity, rigid_body, mass, transform) in &targets {
        if !rigid_body.is_dynamic() {
            continue;
        }
        let p = transform.translation.truncate();
        for (source, accel) in fields.contributions(p, Some(entity)) {
            let force = Mass(mass.value()) * accel;
            if force.value() == Vec2::ZERO {
                continue;
            }
            accumulator.add_force(entity, force);
            accumulator.add_force(source, -force);
        }
    }
}

/// Request to place bodies in orbit around the dominant field source at
/// their position (Algodoo's "set in orbit"): sets each body's velocity to
/// the circular-orbit speed perpendicular to the local field. A physical
/// one-shot like the mouse grab — not undoable, velocities are never
/// authored state.
#[derive(Message, Debug, Clone, Reflect)]
pub struct SetOrbitRequest {
    /// Bodies to set in orbit.
    pub targets: Vec<StableId>,
}

/// Handles [`SetOrbitRequest`]: `v = √(a·r)` about the dominant source,
/// perpendicular to the field, inheriting the source's own velocity.
pub fn set_in_orbit(
    mut requests: MessageReader<SetOrbitRequest>,
    fields: Fields,
    index: Res<IdIndex>,
    transforms: Query<&Transform, With<Body>>,
    mut velocities: Query<&mut LinearVelocity, With<Body>>,
) {
    for request in requests.read() {
        for id in &request.targets {
            let Some(entity) = index.entity(*id) else {
                continue;
            };
            let Ok(p) = transforms.get(entity).map(|t| t.translation.truncate()) else {
                continue;
            };
            let Some((source, accel)) = fields.dominant_at(p, Some(entity)) else {
                continue;
            };
            // Only an *attractive* net pull supports an orbit.
            let Ok(center) = transforms.get(source).map(|t| t.translation.truncate()) else {
                continue;
            };
            let toward = (center - p).normalize_or_zero();
            if toward == Vec2::ZERO || accel.value().dot(toward) <= 0.0 {
                continue;
            }
            let r = Length(p.distance(center));
            let v = orbit_speed(accel.magnitude(), r);
            // Orbit relative to a moving attractor: inherit its drift.
            let drift = velocities.get(source).map(|lv| lv.0).unwrap_or_default();
            if let Ok(mut lv) = velocities.get_mut(entity) {
                lv.0 = drift + toward.perp() * v.value();
            }
        }
    }
}

/// Below this speed the plane friction lets the solver's sleeping take over
/// instead of jittering around zero.
const FRICTION_REST_SPEED: Velocity = Velocity(0.02);

/// Coulomb friction against the back plane (top-down mode).
///
/// With in-plane gravity zeroed, gravity implicitly points "into the
/// screen": every dynamic body presses on the back plane with `m·g` and
/// rubs as it slides/spins. Force `μ·m·g` opposes the velocity; torque
/// `μ·m·g·k` (k = gyration radius) opposes the spin. One-shot forces like
/// the fields above — never authored, undoable, or persisted.
pub fn apply_plane_friction(
    settings: Res<gradiance_domain::settings::SimSettings>,
    bodies: Query<
        (
            Entity,
            &LinearVelocity,
            &AngularVelocity,
            &ComputedMass,
            &ComputedAngularInertia,
        ),
        With<Body>,
    >,
    mut accumulator: ResMut<ForceAccumulator>,
) {
    let mu = settings.plane_friction;
    if mu <= 0.0 {
        return;
    }
    // The implicit into-screen gravity uses the standard magnitude even
    // when in-plane gravity is zeroed (that's what top-down *means*).
    let g = Acceleration(gradiance_core::constants::GRAVITY.length());
    for (entity, linear, angular, mass, inertia) in &bodies {
        let m = mass.value();
        if m <= 0.0 {
            continue;
        }
        let speed = linear.0.length();
        if speed > FRICTION_REST_SPEED.value() {
            let load = mu * m * g.value();
            accumulator.add_force(entity, Force2::new(-linear.0 / speed * load));
        }
        let spin = angular.0;
        if spin.abs() > FRICTION_REST_SPEED.value() {
            // Gyration radius: the lever arm the surface rubs at.
            let k = (inertia.value() / m).max(0.0).sqrt();
            accumulator.add_torque(entity, Torque(-spin.signum() * mu * m * g.value() * k));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_factors_match_algodoo_shapes() {
        // At the surface both are 1; a meter out linear halves, quadratic
        // quarters.
        assert!((falloff_factor(FieldFalloff::Linear, 0.0) - 1.0).abs() < 1e-6);
        assert!((falloff_factor(FieldFalloff::Quadratic, 0.0) - 1.0).abs() < 1e-6);
        let m = 1.0;
        assert!((falloff_factor(FieldFalloff::Linear, m) - 0.5).abs() < 1e-6);
        assert!((falloff_factor(FieldFalloff::Quadratic, m) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn orbit_speed_satisfies_the_limit_cycle_condition() {
        let (a, r) = (Acceleration(250.0), Length(160.0));
        let v = orbit_speed(a, r).value();
        assert!((v * v / r.value() - a.value()).abs() < 1e-3, "a = v²/r");
    }

    #[test]
    fn orbit_speed_holds_at_sub_metre_radii() {
        // SI scale: bodies orbit at tens of centimetres. The old pixel-era
        // `radius.max(1.0)` floor substituted 1 m here and overstated v by 2x.
        let (a, r) = (Acceleration(20.0), Length(0.25));
        let v = orbit_speed(a, r).value();
        assert!(
            (v * v / r.value() - a.value()).abs() < 1e-3,
            "a = v²/r must hold inside 1 m (v = {v})"
        );
    }
}
