//! Air drag and wind become solver forces (brief N-4).
//!
//! Every dynamic body moving through the air (velocity `v`, wind `u`) feels
//! a drag force against its velocity relative to the air, `r = v − u`:
//!
//! ```text
//! F = −(c·w + ½·ρ·Cd·w·|r|) · r
//! ```
//!
//! — linear (viscous) drag `c` plus quadratic (pressure) drag `½ρCd`, both
//! scaled by `w`, the body's **projected width across `r`**. A plate falling
//! face-on presents its whole width, edge-on only its thickness, so it falls
//! slower face-on. The settings are the scene-content [`SimSettings`]
//! fields `air_density`, `drag_coefficient`, `linear_drag` and `wind`.
//!
//! **Panels** (brief Q-6, decision `aero-panel-drag`): the law is applied
//! per edge of the body's convex hull, its derived [`AirPanels`] (built from
//! the polygonized outline when the shape changes,
//! [`crate::body_sync::sync_colliders`], so the per-step pass never contours
//! a shape). Each half of an edge `i` whose outward normal `n̂` leads into
//! its own relative air `rᵢ = v + ω × pᵢ − u` feels
//! `−(Lᵢ/2)·|n̂·r̂ᵢ|·(c + ½ρCd|rᵢ|)·rᵢ` at its midpoint `pᵢ` (two points per
//! edge, so a round body's spin meets air too). For a body that does not spin the facing edges
//! present exactly the hull's width, so the force is the width law above;
//! the torque about the centre of mass turns an asymmetric body into the
//! flow (a weathervane) and the `ω × p` term damps spin (angular drag). A
//! shape symmetric about its centre of mass (a rectangle) feels no torque,
//! so a flat plate still keeps its attitude: flutter needs lift.
//!
//! Like the field and thruster forces the drag is derived: one-shot through
//! avian's [`Forces`], never authored, never in undo records, never saved.
//!
//! In 2D the world is 1 m deep: mass is area × areal density, and the air
//! density is areal too, so `½ρCd·w·v²` is in newtons and a body's terminal
//! speed falling face-on is `√(2mg / (ρ·Cd·w))`.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::settings::SimSettings;
use gradiance_geometry::profile::{DragPanels, WidthProfile};

/// A body's drag panels, the edges of its convex hull in its own frame:
/// derived from its shape by [`crate::body_sync::sync_colliders`] on every
/// shape change; never saved, never undone, never read by commands.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AirPanels(pub DragPanels);

/// A body's state, as the air sees it, for [`DragLaw::load`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirMotion {
    /// Linear velocity of the centre of mass (m/s).
    pub velocity: Vec2,
    /// Angular velocity (rad/s, counter-clockwise).
    pub spin: f32,
    /// Rotation (radians).
    pub rotation: f32,
    /// The centre of mass in the body's frame (m).
    pub center_of_mass: Vec2,
    /// Mass (kg); 0 disables the step caps.
    pub mass: f32,
    /// Angular inertia about the centre of mass (kg·m²).
    pub inertia: f32,
}

/// The drag coefficients one step uses, read once from [`SimSettings`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragLaw {
    /// Linear drag per metre of width (N·s/m²).
    pub linear: f32,
    /// `½ ρ Cd` (kg/m²): quadratic drag per metre of width.
    pub half_rho_cd: f32,
    /// The air's velocity (m/s).
    pub wind: Vec2,
}

impl DragLaw {
    /// The law the settings describe. Negative or non-finite coefficients
    /// read as 0 (drag never pushes a body along), a non-finite wind as calm.
    #[must_use]
    pub fn from_settings(settings: &SimSettings) -> Self {
        let sane = |x: f32| if x.is_finite() { x.max(0.0) } else { 0.0 };
        let wind = settings.wind.value();
        Self {
            linear: sane(settings.linear_drag),
            half_rho_cd: 0.5 * sane(settings.air_density.value()) * sane(settings.drag_coefficient),
            wind: if wind.is_finite() { wind } else { Vec2::ZERO },
        }
    }

    /// Whether this law exerts any force.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.linear > 0.0 || self.half_rho_cd > 0.0
    }

    /// The width law: the drag force on a body of mass `mass` moving at
    /// `velocity`, whose profile is `profile` and which is turned by
    /// `rotation` radians, over a step of `dt` seconds. Pure. What
    /// [`load`](Self::load) gives a body that does not spin (the system
    /// applies `load`).
    ///
    /// The force is limited so one step cannot more than stop the body
    /// relative to the air (`|F|·dt ≤ m·|r|`): explicit drag with a big
    /// coefficient on a light body would otherwise overshoot and blow up.
    /// The limit binds only where the drag time constant is shorter than a
    /// step, far from any terminal-velocity regime a test measures.
    #[must_use]
    pub fn force(
        &self,
        profile: &WidthProfile,
        velocity: Vec2,
        rotation: f32,
        mass: f32,
        dt: f32,
    ) -> Vec2 {
        let relative = velocity - self.wind;
        let speed = relative.length();
        if !speed.is_finite() || speed <= f32::EPSILON {
            return Vec2::ZERO;
        }
        // The flow direction in the body's frame picks the width.
        let local = Vec2::from_angle(-rotation).rotate(relative);
        let width = profile.width_across(local);
        let mut coefficient = width * (self.linear + self.half_rho_cd * speed);
        if mass > 0.0 && dt > 0.0 {
            coefficient = coefficient.min(mass / dt);
        }
        -relative * coefficient
    }
}

impl DragLaw {
    /// The drag force (at the centre of mass) and torque (about it) on a
    /// body with `panels` in `motion`, over a step of `dt` seconds. Pure.
    ///
    /// Caps: the force so one step cannot more than stop the centre of mass
    /// relative to the air (`|F|·dt ≤ m·|r|`), and a torque that opposes the
    /// spin so one step cannot more than stop it (`|τ|·dt ≤ I·|ω|`). A
    /// torque that turns a body into the flow is the force's own moment and
    /// needs no cap.
    #[must_use]
    pub fn load(&self, panels: &DragPanels, motion: &AirMotion, dt: f32) -> (Vec2, f32) {
        let turn = Vec2::from_angle(motion.rotation);
        let (mut force, mut torque) = (Vec2::ZERO, 0.0_f32);
        // Each edge is two half-edges, each pressed at its own midpoint: a
        // spinning edge leads with one half and trails with the other, so a
        // round body (whose edge midpoints only see tangential air) still
        // feels its spin. A still body's halves see the same air: the sum is
        // the width law unchanged.
        for panel in panels.panels() {
            let normal = turn.rotate(panel.normal);
            let along = normal.perp() * (panel.length * 0.25);
            let mid = turn.rotate(panel.mid - motion.center_of_mass);
            for arm in [mid - along, mid + along] {
                let relative = motion.velocity + motion.spin * arm.perp() - self.wind;
                let speed = relative.length();
                if !speed.is_finite() || speed <= f32::EPSILON {
                    continue;
                }
                let facing = normal.dot(relative) / speed;
                if facing <= 0.0 {
                    continue;
                }
                let push = -relative
                    * (0.5 * panel.length * facing * (self.linear + self.half_rho_cd * speed));
                force += push;
                torque += arm.perp_dot(push);
            }
        }
        if motion.mass > 0.0 && dt > 0.0 {
            let limit = motion.mass * (motion.velocity - self.wind).length() / dt;
            let size = force.length();
            if size > limit {
                force *= limit / size;
            }
        }
        if motion.inertia > 0.0 && dt > 0.0 && torque * motion.spin < 0.0 {
            let limit = motion.inertia * motion.spin.abs() / dt;
            torque = torque.clamp(-limit, limit);
        }
        (force, torque)
    }
}

/// **The drag pass:** one system over the dynamic bodies, once per physics
/// step (`FixedUpdate`, before avian's step). Returns at once while both drag
/// terms are 0 (the default), so a scene without drag pays one resource read.
///
/// Cost: `O(B · P)` for `B` bodies of at most `P ≤ 64` panels — a few
/// multiplies and one square root per panel; no allocation, no contour.
pub fn apply_air_drag(
    settings: Res<SimSettings>,
    fixed: Res<Time<Fixed>>,
    physics: Res<Time<Physics>>,
    mut bodies: Query<
        (
            Forces,
            &RigidBody,
            &ComputedMass,
            &ComputedAngularInertia,
            &ComputedCenterOfMass,
            &AirPanels,
        ),
        With<Body>,
    >,
) {
    let law = DragLaw::from_settings(&settings);
    if !law.is_active() {
        return;
    }
    // The step avian is about to take: the fixed timestep at the sim speed.
    let dt = fixed.timestep().as_secs_f32() * physics.relative_speed();
    if dt <= 0.0 {
        return;
    }
    for (mut forces, rigid_body, mass, inertia, com, panels) in &mut bodies {
        if !rigid_body.is_dynamic() {
            continue;
        }
        let motion = AirMotion {
            velocity: forces.linear_velocity(),
            spin: forces.angular_velocity(),
            rotation: forces.rotation().as_radians(),
            center_of_mass: com.0,
            mass: mass.value(),
            inertia: inertia.value(),
        };
        let (force, torque) = law.load(&panels.0, &motion, dt);
        if force != Vec2::ZERO {
            forces.apply_force(force);
        }
        if torque != 0.0 {
            forces.apply_torque(torque);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_units::{Density, Velocity2};

    fn plate() -> WidthProfile {
        WidthProfile::from_points(&[
            Vec2::new(-1.0, -0.05),
            Vec2::new(1.0, -0.05),
            Vec2::new(1.0, 0.05),
            Vec2::new(-1.0, 0.05),
        ])
    }

    fn law(cd: f32, linear: f32, wind: Vec2) -> DragLaw {
        DragLaw::from_settings(&SimSettings {
            air_density: Density(1.2),
            drag_coefficient: cd,
            linear_drag: linear,
            wind: Velocity2(wind),
            ..SimSettings::default()
        })
    }

    #[test]
    fn the_default_settings_exert_no_drag() {
        assert!(!DragLaw::from_settings(&SimSettings::default()).is_active());
    }

    #[test]
    fn quadratic_drag_is_half_rho_cd_w_v_squared_against_the_motion() {
        let f = law(1.0, 0.0, Vec2::ZERO).force(&plate(), Vec2::new(0.0, -3.0), 0.0, 0.0, 0.0);
        // ½ · 1.2 · 1 · 2 m · 3² = 10.8 N, upward.
        assert!((f - Vec2::new(0.0, 10.8)).length() < 1e-3, "{f}");
    }

    #[test]
    fn an_edge_on_plate_feels_its_thickness_only() {
        let face = law(1.0, 0.0, Vec2::ZERO).force(&plate(), Vec2::NEG_Y, 0.0, 0.0, 0.0);
        // Turned a quarter, the same fall meets the 0.1 m edge.
        let edge = law(1.0, 0.0, Vec2::ZERO).force(
            &plate(),
            Vec2::NEG_Y,
            core::f32::consts::FRAC_PI_2,
            0.0,
            0.0,
        );
        assert!((face.length() / edge.length() - 20.0).abs() < 0.01);
    }

    #[test]
    fn linear_drag_is_c_w_v() {
        let f = law(0.0, 0.5, Vec2::ZERO).force(&plate(), Vec2::new(0.0, -4.0), 0.0, 0.0, 0.0);
        // 0.5 · 2 m · 4 m/s = 4 N.
        assert!((f - Vec2::new(0.0, 4.0)).length() < 1e-4, "{f}");
    }

    #[test]
    fn drag_acts_on_the_velocity_relative_to_the_wind() {
        let wind = Vec2::new(0.0, 2.0);
        let l = law(1.0, 0.0, wind);
        assert_eq!(l.force(&plate(), wind, 0.0, 0.0, 0.0), Vec2::ZERO);
        // At rest in the wind: pushed along it.
        assert!(l.force(&plate(), Vec2::ZERO, 0.0, 0.0, 0.0).y > 0.0);
    }

    #[test]
    fn one_step_never_reverses_the_relative_velocity() {
        let l = law(1.0e6, 0.0, Vec2::ZERO);
        let (mass, dt, v) = (0.01, 1.0 / 60.0, Vec2::new(5.0, 0.0));
        let f = l.force(&plate(), v, 0.0, mass, dt);
        assert!((v + f / mass * dt).length() <= 1e-4, "{f}");
    }

    fn panels(points: &[Vec2]) -> DragPanels {
        DragPanels::from_points(points)
    }

    fn still(velocity: Vec2, rotation: f32) -> AirMotion {
        AirMotion {
            velocity,
            spin: 0.0,
            rotation,
            center_of_mass: Vec2::ZERO,
            mass: 0.0,
            inertia: 0.0,
        }
    }

    fn plate_points() -> [Vec2; 4] {
        [
            Vec2::new(-1.0, -0.05),
            Vec2::new(1.0, -0.05),
            Vec2::new(1.0, 0.05),
            Vec2::new(-1.0, 0.05),
        ]
    }

    #[test]
    fn a_still_body_feels_the_width_law_through_its_panels() {
        let l = law(1.0, 0.3, Vec2::new(0.5, 0.0));
        for k in 0..64 {
            let angle = core::f32::consts::PI * k as f32 / 64.0;
            let v = Vec2::from_angle(angle * 1.7) * 3.0;
            let (f, _) = l.load(&panels(&plate_points()), &still(v, angle), 0.0);
            let g = l.force(&plate(), v, angle, 0.0, 0.0);
            assert!(
                (f - g).length() <= 1e-3 * (1.0 + g.length()),
                "{k}: {f} vs {g}"
            );
        }
    }

    #[test]
    fn a_symmetric_body_feels_no_torque_at_any_angle() {
        let l = law(1.0, 0.2, Vec2::ZERO);
        let hexagon: Vec<Vec2> = (0..6)
            .map(|i| Vec2::from_angle(i as f32 * core::f32::consts::TAU / 6.0))
            .collect();
        for shape in [panels(&plate_points()), panels(&hexagon)] {
            for k in 0..64 {
                let angle = core::f32::consts::TAU * k as f32 / 64.0;
                let (_, torque) = l.load(&shape, &still(Vec2::new(0.3, -2.0), angle), 0.0);
                assert!(torque.abs() < 1e-4, "{k}: {torque}");
            }
        }
    }

    #[test]
    fn a_heavy_nose_turns_into_the_flow() {
        // A 2 m bar whose centre of mass sits 0.6 m toward its +x end,
        // falling with the bar level: the tail (−x) has more area behind
        // the centre of mass, so the drag turns the nose (+x) down.
        let l = law(1.0, 0.0, Vec2::ZERO);
        let motion = AirMotion {
            center_of_mass: Vec2::new(0.6, 0.0),
            ..still(Vec2::new(0.0, -5.0), 0.0)
        };
        let (_, torque) = l.load(&panels(&plate_points()), &motion, 0.0);
        // Clockwise (negative): the tail (−x) rises, the nose (+x) drops.
        assert!(torque < 0.0, "the tail lifts, the nose drops: {torque}");
    }

    #[test]
    fn spin_in_still_air_is_damped_but_never_reversed() {
        let l = law(1.0, 0.0, Vec2::ZERO);
        let spinning = AirMotion {
            spin: 4.0,
            ..still(Vec2::ZERO, 0.0)
        };
        let (force, torque) = l.load(&panels(&plate_points()), &spinning, 0.0);
        assert!(force.length() < 1e-4, "no net force: {force}");
        assert!(torque < 0.0, "against the spin: {torque}");
        // A huge drag on a light body: one step stops the spin, no more.
        let l = law(1.0e6, 0.0, Vec2::ZERO);
        let light = AirMotion {
            mass: 0.01,
            inertia: 0.001,
            ..spinning
        };
        let dt = 1.0 / 60.0;
        let (_, torque) = l.load(&panels(&plate_points()), &light, dt);
        assert!(light.spin + torque / light.inertia * dt >= -1e-4);
    }

    #[test]
    fn bad_settings_read_as_no_drag() {
        let l = DragLaw::from_settings(&SimSettings {
            drag_coefficient: -1.0,
            linear_drag: f32::NAN,
            wind: Velocity2(Vec2::new(f32::INFINITY, 0.0)),
            ..SimSettings::default()
        });
        assert!(!l.is_active());
        assert_eq!(l.wind, Vec2::ZERO);
    }
}
