//! Buoyancy: fluid regions push up what they hold (brief N-7).
//!
//! A body carrying the authored [`Fluid`] is a region of fluid of areal
//! density `ρ` and drag rate `k`. Every dynamic body overlapping it, with a
//! submerged area `A` whose centroid is `c`, feels at `c`
//!
//! ```text
//! F = −ρ·A·g  −  ρ·k·A·(v_c − v_fluid)
//! ```
//!
//! — Archimedes' push (the weight of the displaced fluid, against gravity)
//! plus a linear drag against the motion of the submerged centroid through
//! the fluid — and a torque `−ρ·k·A·(I/m)·ω` against its spin. A body as
//! dense as the fluid, wholly submerged, therefore loses speed and spin at
//! the rate `k`. Pushing at `c` rather than the centre of mass is what
//! rights a boat: a tilted hull's submerged centroid moves to the low side.
//!
//! In 2D the world is 1 m deep and densities are areal, so a body of density
//! `ρ_b` floats with the fraction `ρ_b / ρ` of its area submerged.
//!
//! `A` and `c` are exact for the polygons: the body's derived
//! [`SubmergeOutline`] (built with its collider, from the same polygonized
//! contours) clipped to the fluid's derived [`FluidRegion`] (its convex
//! pieces, built when the fluid's shape changes), by the pure clip in
//! `geometry::submerge`. The per-step pass never contours a shape.
//!
//! The forces are derived, like the field, thruster and drag forces:
//! one-shot through avian's [`Forces`], never authored, never in undo
//! records, never saved. Fluids act on dynamic bodies that are not fluids
//! themselves; a fluid body feels no reaction (it is the medium, usually a
//! static sensor). Overlapping fluid regions add up.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::fluid::Fluid;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::submerge::{
    AreaMoment, ClipScratch, FluidVolume, SolidOutline, transform_ring_into,
};

/// A fluid body's region in its own frame, as convex pieces: derived from
/// its shape by [`sync_fluid_regions`] when the shape or the fluid changes;
/// never saved, never undone, never read by commands.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct FluidRegion(pub FluidVolume);

/// A body's outline for the submerged clip, in its own frame: derived from
/// its shape by [`crate::body_sync::sync_colliders`], beside the collider;
/// never saved, never undone, never read by commands. Empty for a shape
/// that holds a half-plane (the ground floats on nothing).
#[derive(Component, Debug, Clone, PartialEq, Default)]
pub struct SubmergeOutline(pub SolidOutline);

/// The fluid region a shape holds: the open sea under a half-plane's
/// surface, otherwise the convex pieces of its polygonized components.
#[must_use]
pub fn fluid_volume_for(shape: &ShapeDef) -> FluidVolume {
    match shape {
        ShapeDef::HalfPlane => FluidVolume::half_plane(),
        _ => FluidVolume::from_components(&gradiance_geometry::polygonize::polygonize_components(
            shape,
        )),
    }
}

/// Derives [`FluidRegion`] for every fluid body whose fluid was added or
/// whose shape changed, and drops it from a body that stopped being a fluid.
#[expect(clippy::type_complexity, reason = "one filtered query")]
pub fn sync_fluid_regions(
    mut commands: Commands,
    changed: Query<
        (Entity, &ShapeDef),
        (
            With<Body>,
            With<Fluid>,
            Or<(Added<Fluid>, Changed<ShapeDef>)>,
        ),
    >,
    mut removed: RemovedComponents<Fluid>,
    still: Query<(), With<Fluid>>,
) {
    for entity in removed.read() {
        if still.get(entity).is_err()
            && let Ok(mut e) = commands.get_entity(entity)
        {
            e.remove::<FluidRegion>();
        }
    }
    for (entity, shape) in &changed {
        commands
            .entity(entity)
            .insert(FluidRegion(fluid_volume_for(shape)));
    }
}

/// The force and torque one fluid exerts on one body, from the submerged
/// moment and the body's state. Pure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Immersion {
    /// Areal fluid density (kg/m²).
    pub density: f32,
    /// Drag rate (1/s).
    pub drag: f32,
    /// Gravity (m/s²).
    pub gravity: Vec2,
    /// The fluid's own velocity (m/s; a moving tank carries its water).
    pub flow: Vec2,
}

/// A body's motion state, as the fluid sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyMotion {
    /// World centre of mass.
    pub center_of_mass: Vec2,
    /// Linear velocity (m/s).
    pub velocity: Vec2,
    /// Angular velocity (rad/s).
    pub spin: f32,
    /// Mass (kg).
    pub mass: f32,
    /// Angular inertia about the centre of mass (kg·m²).
    pub inertia: f32,
}

impl Immersion {
    /// The force (applied at the returned world point) and the torque on a
    /// body with `submerged` area moment (world frame), over a step of `dt`
    /// seconds. `None` when nothing is submerged.
    ///
    /// The drag is limited so one step cannot more than stop the body
    /// relative to the fluid (`ρ·k·A·dt ≤ m`, and likewise for the spin):
    /// explicit drag on a light body in a thick fluid would otherwise
    /// overshoot and blow up. Buoyancy itself is never limited.
    #[must_use]
    pub fn push(
        &self,
        submerged: AreaMoment,
        body: &BodyMotion,
        dt: f32,
    ) -> Option<(Vec2, Vec2, f32)> {
        let centroid = submerged.centroid()?;
        let area = submerged.area;
        let buoyancy = -self.gravity * (self.density * area);
        let mut linear = self.density * self.drag * area;
        let gyration = if body.mass > 0.0 {
            (body.inertia / body.mass).max(0.0)
        } else {
            0.0
        };
        let mut angular = linear * gyration;
        if dt > 0.0 {
            if body.mass > 0.0 {
                linear = linear.min(body.mass / dt);
            }
            if body.inertia > 0.0 {
                angular = angular.min(body.inertia / dt);
            }
        }
        let arm = centroid - body.center_of_mass;
        let at_centroid = body.velocity + arm.perp() * body.spin;
        let drag = -(at_centroid - self.flow) * linear;
        let force = buoyancy + drag;
        let torque = -body.spin * angular;
        (force.is_finite() && torque.is_finite()).then_some((force, centroid, torque))
    }
}

/// Reused buffers for the buoyancy pass: they grow to the largest body once
/// and are then reused, so the steady state allocates nothing.
#[derive(Default)]
pub struct BuoyancyScratch {
    rings: Vec<Vec<Vec2>>,
    clip: ClipScratch,
}

/// **The buoyancy pass:** one system, once per physics step (`FixedUpdate`,
/// before avian's step). Returns at once when the scene has no fluid.
///
/// Cost: `O(F · B)` circle-against-box rejects for `F` fluids and `B`
/// dynamic bodies (a few multiplies each), then, per overlapping pair,
/// `O(P · V)` for the clip — `P` the fluid's convex pieces (1 for a box or
/// any convex tank, 1 for a sea), `V` the body's outline vertices (4 for a
/// box, 48 for a circle). No allocation once the scratch has grown, no
/// contour.
///
/// A pushed body is also **woken**: a floater settles by bobbing toward its
/// waterline, and a body left asleep mid-bob freezes at whatever depth it
/// had reached (sleeping skips the very forces that were still converging
/// it). Submerged bodies therefore never sleep — the price of an
/// equilibrium that stays honest. Fields and air drag keep `Box2D`'s
/// constant-of-the-scene semantics instead: they do not wake, so a body
/// hovering in a field at rest simply stays put.
#[expect(clippy::type_complexity, reason = "two filtered queries")]
pub fn apply_buoyancy(
    mut commands: Commands,
    gravity: Res<Gravity>,
    fixed: Res<Time<Fixed>>,
    physics: Res<Time<Physics>>,
    fluids: Query<
        (
            &Fluid,
            &FluidRegion,
            &Position,
            &Rotation,
            Option<&LinearVelocity>,
        ),
        With<Body>,
    >,
    mut bodies: Query<
        (
            Entity,
            Forces,
            &RigidBody,
            &ComputedMass,
            &ComputedAngularInertia,
            &ComputedCenterOfMass,
            &SubmergeOutline,
        ),
        (With<Body>, Without<Fluid>),
    >,
    mut scratch: Local<BuoyancyScratch>,
) {
    if fluids.is_empty() {
        return;
    }
    let dt = fixed.timestep().as_secs_f32() * physics.relative_speed();
    let BuoyancyScratch { rings, clip } = &mut *scratch;
    for (fluid, region, fluid_pos, fluid_rot, flow) in &fluids {
        let fluid = fluid.sanitized();
        if fluid.density.value() <= 0.0 || region.0.is_empty() {
            continue;
        }
        let immersion = Immersion {
            density: fluid.density.value(),
            drag: fluid.drag,
            gravity: gravity.0,
            flow: flow.map_or(Vec2::ZERO, |v| v.0),
        };
        let to_world = Vec2::from_angle(fluid_rot.as_radians());
        let to_fluid = Vec2::new(to_world.x, -to_world.y);
        for (entity, mut forces, rigid_body, mass, inertia, com, outline) in &mut bodies {
            if !rigid_body.is_dynamic() || outline.0.is_empty() {
                continue;
            }
            // The cheap reject: the body's bounding circle against the
            // region's box (or a sea's surface), in the fluid's frame.
            let center = to_fluid.rotate(forces.position() - fluid_pos.0);
            if !region.0.may_touch(center, outline.0.radius()) {
                continue;
            }
            let body_rot = Vec2::from_angle(forces.rotation().as_radians());
            let rotation = to_fluid.rotate(body_rot);
            let mut count = 0;
            for ring in outline.0.rings() {
                if rings.len() == count {
                    rings.push(Vec::new());
                }
                transform_ring_into(ring, rotation, center, &mut rings[count]);
                count += 1;
            }
            let local = region
                .0
                .submerged(rings[..count].iter().map(Vec::as_slice), clip);
            let submerged = AreaMoment {
                area: local.area,
                first: to_world.rotate(local.first) + fluid_pos.0 * local.area,
            };
            let motion = BodyMotion {
                center_of_mass: forces.position() + body_rot.rotate(com.0),
                velocity: forces.linear_velocity(),
                spin: forces.angular_velocity(),
                mass: mass.value(),
                inertia: inertia.value(),
            };
            if let Some((force, point, torque)) = immersion.push(submerged, &motion, dt) {
                forces.apply_force_at_point(force, point);
                forces.apply_torque(torque);
                // See the system docs: a submerged body stays awake so the
                // bobbing converges instead of freezing off-waterline.
                commands.queue(WakeBody(entity));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn water() -> Immersion {
        Immersion {
            density: 2.0,
            drag: 1.0,
            gravity: Vec2::new(0.0, -10.0),
            flow: Vec2::ZERO,
        }
    }

    fn at_rest(mass: f32) -> BodyMotion {
        BodyMotion {
            center_of_mass: Vec2::ZERO,
            velocity: Vec2::ZERO,
            spin: 0.0,
            mass,
            inertia: mass / 6.0,
        }
    }

    #[test]
    fn the_push_is_the_weight_of_the_displaced_fluid_at_the_centroid() {
        let submerged = AreaMoment {
            area: 0.5,
            first: Vec2::new(0.0, -0.25) * 0.5,
        };
        let (force, point, torque) = water().push(submerged, &at_rest(1.0), 0.0).expect("a push");
        // ρ A g = 2 · 0.5 · 10 = 10 N up.
        assert!((force - Vec2::new(0.0, 10.0)).length() < 1e-5, "{force}");
        assert!((point - Vec2::new(0.0, -0.25)).length() < 1e-6);
        assert!(torque.abs() < 1e-6);
    }

    #[test]
    fn nothing_submerged_is_no_push() {
        assert!(
            water()
                .push(AreaMoment::default(), &at_rest(1.0), 0.0)
                .is_none()
        );
    }

    #[test]
    fn drag_opposes_motion_relative_to_the_flow_and_spin() {
        let submerged = AreaMoment {
            area: 1.0,
            first: Vec2::ZERO,
        };
        let mut body = at_rest(2.0);
        body.velocity = Vec2::new(3.0, 0.0);
        body.spin = 1.5;
        let still = Immersion {
            gravity: Vec2::ZERO,
            ..water()
        };
        let (force, _, torque) = still.push(submerged, &body, 0.0).expect("a push");
        // ρ k A v = 2 · 1 · 1 · 3 = 6 N against the motion.
        assert!((force - Vec2::new(-6.0, 0.0)).length() < 1e-5, "{force}");
        // ρ k A (I/m) ω = 2 · (1/6) · 1.5.
        assert!((torque + 0.5).abs() < 1e-5, "{torque}");
        // In a current at the body's own speed there is no drag.
        let current = Immersion {
            flow: body.velocity,
            ..still
        };
        body.spin = 0.0;
        let (force, ..) = current.push(submerged, &body, 0.0).expect("a push");
        assert!(force.length() < 1e-6, "{force}");
    }

    #[test]
    fn one_step_of_drag_never_reverses_the_motion() {
        let thick = Immersion {
            drag: 1.0e6,
            gravity: Vec2::ZERO,
            ..water()
        };
        let submerged = AreaMoment {
            area: 1.0,
            first: Vec2::ZERO,
        };
        let mut body = at_rest(0.1);
        body.velocity = Vec2::new(5.0, 0.0);
        body.spin = 2.0;
        let dt = 1.0 / 60.0;
        let (force, _, torque) = thick.push(submerged, &body, dt).expect("a push");
        assert!((body.velocity + force / body.mass * dt).length() <= 1e-4);
        assert!((body.spin + torque / body.inertia * dt).abs() <= 1e-4);
    }
}
