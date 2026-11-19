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
//! slower face-on. The width comes from the body's derived [`AirProfile`],
//! sampled from the polygonized outline when the shape changes
//! ([`crate::body_sync::sync_colliders`]), so the per-step pass never
//! contours a shape. The settings are the scene-content [`SimSettings`]
//! fields `air_density`, `drag_coefficient`, `linear_drag` and `wind`.
//!
//! The force acts at the centre of mass: drag never turns a body (no
//! aerodynamic torque, no angular drag). Like the field and thruster forces
//! it is derived: one-shot through avian's [`Forces`], never authored, never
//! in undo records, never saved.
//!
//! In 2D the world is 1 m deep: mass is area × areal density, and the air
//! density is areal too, so `½ρCd·w·v²` is in newtons and a body's terminal
//! speed falling face-on is `√(2mg / (ρ·Cd·w))`.

use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::settings::SimSettings;
use gradiance_geometry::profile::WidthProfile;

/// A body's projected width over every direction, in its own frame: derived
/// from its shape by [`crate::body_sync::sync_colliders`] on every shape
/// change; never saved, never undone, never read by commands.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct AirProfile(pub WidthProfile);

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

    /// The drag force on a body of mass `mass` moving at `velocity`, whose
    /// profile is `profile` and which is turned by `rotation` radians, over
    /// a step of `dt` seconds. Pure.
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

/// **The drag pass:** one system over the dynamic bodies, once per physics
/// step (`FixedUpdate`, before avian's step). Returns at once while both drag
/// terms are 0 (the default), so a scene without drag pays one resource read.
///
/// Cost: `O(B)` for `B` bodies with a profile — per dynamic body one
/// `atan2`, one table lerp and a few multiplies; no allocation, no contour.
#[expect(clippy::type_complexity, reason = "one filtered query")]
pub fn apply_air_drag(
    settings: Res<SimSettings>,
    fixed: Res<Time<Fixed>>,
    physics: Res<Time<Physics>>,
    mut bodies: Query<(Forces, &RigidBody, &ComputedMass, &AirProfile), With<Body>>,
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
    for (mut forces, rigid_body, mass, profile) in &mut bodies {
        if !rigid_body.is_dynamic() {
            continue;
        }
        let force = law.force(
            &profile.0,
            forces.linear_velocity(),
            forces.rotation().as_radians(),
            mass.value(),
            dt,
        );
        if force != Vec2::ZERO {
            forces.apply_force(force);
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
