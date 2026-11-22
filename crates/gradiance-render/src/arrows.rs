//! Per-body motion arrows (brief S-2b): a body whose `Appearance.arrows`
//! asks for them shows its velocity, momentum and net force as arrows from
//! its centre of mass.
//!
//! Read-only and derived: the velocity and mass come through
//! `physics::queries`, the force is `m·a` from the derived
//! `physics::acceleration::BodyAcceleration`. Nothing here is saved; the
//! flags are the authored half. The length is the workstation setting
//! `CameraSettings::arrow_scale` (metres per unit).

use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::{Appearance, BodyArrows};
use gradiance_domain::depth::DepthBand;
use gradiance_domain::settings::CameraSettings;
use gradiance_domain::theme::{ARROW_FORCE, ARROW_MOMENTUM, ARROW_VELOCITY};
use gradiance_interaction::overlay::OverlayGizmos;
use gradiance_physics::acceleration::BodyAcceleration;
use gradiance_physics::queries::PhysicsQueries;

/// Shorter than this (m) an arrow is not drawn: a body at rest shows none.
const MIN_ARROW: f32 = 1e-3;

/// The arrows `flags` asks for, as `(vector, which)` in the order velocity,
/// momentum, force, for a body of `mass` moving at `velocity` with
/// `acceleration`. Pure.
pub fn arrow_vectors(
    flags: BodyArrows,
    velocity: Vec2,
    mass: f32,
    acceleration: Vec2,
) -> impl Iterator<Item = (Vec2, Arrow)> {
    [
        (flags.velocity, velocity, Arrow::Velocity),
        (flags.momentum, velocity * mass, Arrow::Momentum),
        (flags.force, acceleration * mass, Arrow::Force),
    ]
    .into_iter()
    .filter(|(on, ..)| *on)
    .map(|(_, v, which)| (v, which))
}

/// Which motion arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    /// The velocity.
    Velocity,
    /// The momentum.
    Momentum,
    /// The net force.
    Force,
}

impl Arrow {
    /// The depth lift of this kind (m): later kinds draw over earlier ones.
    const fn layer(self) -> f32 {
        match self {
            Self::Velocity => 0.0,
            Self::Momentum => 0.002,
            Self::Force => 0.004,
        }
    }

    fn color(self) -> Color {
        Color::from(match self {
            Self::Velocity => ARROW_VELOCITY,
            Self::Momentum => ARROW_MOMENTUM,
            Self::Force => ARROW_FORCE,
        })
    }
}

/// Draws every body's requested arrows. `O(B)` over the bodies, allocation
/// free; a body with no arrow on costs one flag test.
pub fn draw_body_arrows(
    bodies: Query<(Entity, &Appearance, &Transform, &DepthBand), With<Body>>,
    accelerations: Query<&BodyAcceleration>,
    centers: Query<&avian2d::prelude::ComputedCenterOfMass>,
    physics: PhysicsQueries,
    camera: Option<Res<CameraSettings>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let scale = camera.map_or(CameraSettings::default().arrow_scale, |c| {
        c.sanitized().arrow_scale
    });
    for (entity, appearance, transform, band) in &bodies {
        if appearance.arrows.is_none() {
            continue;
        }
        let Some((velocity, _)) = physics.velocity_of(entity) else {
            continue;
        };
        let mass = physics
            .mass_of(entity)
            .map_or(0.0, gradiance_units::Mass::value);
        let accel = accelerations.get(entity).map_or(Vec2::ZERO, |a| a.value);
        let com = centers.get(entity).map_or(Vec2::ZERO, |c| c.0);
        let origin =
            transform.translation.truncate() + (transform.rotation * com.extend(0.0)).truncate();
        let z = band.z_front() + transform.translation.z + 0.01;
        for (vector, which) in arrow_vectors(appearance.arrows, velocity.value(), mass, accel) {
            let tip = vector * scale;
            if tip.length() < MIN_ARROW {
                continue;
            }
            // Each kind a hair nearer than the one before, so a short
            // momentum arrow along the velocity draws on top of it.
            let z = z + which.layer();
            gizmos.arrow(origin.extend(z), (origin + tip).extend(z), which.color());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_asked_arrows_are_drawn() {
        let flags = BodyArrows {
            velocity: true,
            momentum: false,
            force: true,
        };
        let got: Vec<_> =
            arrow_vectors(flags, Vec2::new(2.0, 0.0), 3.0, Vec2::new(0.0, -10.0)).collect();
        assert_eq!(
            got,
            [
                (Vec2::new(2.0, 0.0), Arrow::Velocity),
                (Vec2::new(0.0, -30.0), Arrow::Force)
            ]
        );
        assert_eq!(
            arrow_vectors(BodyArrows::default(), Vec2::X, 1.0, Vec2::X).count(),
            0
        );
    }
}
