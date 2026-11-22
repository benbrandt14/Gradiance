//! Conveyor surfaces (brief S-8): the collision hook that gives a contact
//! the surface speed of the bodies in it.
//!
//! avian 0.8 has a per-manifold `tangent_speed`, the relative speed the
//! friction solver drives the two surfaces to. The hook sets it from each
//! body's authored [`SurfaceSpeed`]. Friction does the carrying, so a load
//! on a slippery belt slides, as on a real one. Nothing here is a force.
//!
//! # The sign
//!
//! avian's 2D tangent is the normal (from shape 1 to shape 2) turned
//! clockwise, and the solver drives `(v2 - v1) · t` to `-tangent_speed`.
//! A body's counter-clockwise outline direction at the contact is its
//! outward normal turned counter-clockwise. For the body as shape 1 that
//! is `-t`; for the body as shape 2 (normal reversed) it is `t`. In both
//! cases a surface speed `S` adds `+S` to `tangent_speed`.

use avian2d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use gradiance_domain::surface::SurfaceSpeed;

/// The collision hooks of the app: the conveyor surface speed.
#[derive(SystemParam)]
pub struct GradianceHooks<'w, 's> {
    speeds: Query<'w, 's, &'static SurfaceSpeed>,
}

impl GradianceHooks<'_, '_> {
    /// The surface speed of a collider, or of the body it belongs to.
    fn speed(&self, collider: Entity, body: Option<Entity>) -> f32 {
        self.speeds
            .get(collider)
            .ok()
            .or_else(|| body.and_then(|b| self.speeds.get(b).ok()))
            .map_or(0.0, |s| s.0.0)
    }
}

impl CollisionHooks for GradianceHooks<'_, '_> {
    fn modify_contacts(&self, contacts: &mut ContactPair, _commands: &mut Commands) -> bool {
        let speed = self.speed(contacts.collider1, contacts.body1)
            + self.speed(contacts.collider2, contacts.body2);
        for manifold in &mut contacts.manifolds {
            manifold.tangent_speed = speed;
        }
        true
    }
}

/// Opts every body into the contact hook.
///
/// avian 0.8 reads `ActiveCollisionHooks::MODIFY_CONTACTS` only when a
/// collider's broad-phase proxy is made: its insert observer updates the
/// filter flag but not the modify flag, so a flag added to a body that
/// already collides is never seen (`docs/upstream-watch.md`). Thus the flag
/// is a required component of every body, and a body without a surface
/// speed adds 0. The hook is two lookups per touching pair.
pub(crate) fn install(app: &mut App) {
    app.register_required_components_with::<gradiance_domain::Body, ActiveCollisionHooks>(|| {
        ActiveCollisionHooks::MODIFY_CONTACTS
    });
}
