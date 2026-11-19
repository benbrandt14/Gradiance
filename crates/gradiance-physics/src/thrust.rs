//! Thrusters become solver forces (brief N-3).
//!
//! A thruster node ([`NodeKind::Thruster`]) attached to a body pushes it
//! along the thruster's axis (in the body's frame) at the node's point: the
//! node's [`NodeAttachment`] offset, in the body's frame. Off the centre of
//! mass the push also turns the body (avian's `apply_force_at_point`).
//!
//! The push is derived, like the field forces: one-shot through avian's
//! [`Forces`], never authored, never in undo records, never saved. A signal
//! drive ([`SignalDrive::thrust`], brief N-1) replaces the authored thrust
//! while a binding sets it.
//!
//! [`apply_thrusters`] runs in `FixedUpdate`, once per physics step, so the
//! impulse a thruster gives over a span is the same however the frames fall
//! on the steps (a one-shot force applied in `Update` would be spent by one
//! step and missed or doubled by the others).

use crate::drive::SignalDrive;
use avian2d::prelude::*;
use bevy::prelude::*;
use gradiance_core::ids::IdIndex;
use gradiance_domain::Body;
use gradiance_domain::node::{BehaviorNode, NodeAttachment, NodeKind};

/// The world force and the world point a thruster pushes at, for a body at
/// `body_pos` rotated by `body_rot` radians. `thrust` is the authored thrust
/// or a drive's. `None` when there is nothing to push (a zero or non-finite
/// thrust). Pure.
#[must_use]
pub fn thruster_push(
    thruster: &gradiance_domain::thruster::Thruster,
    attachment: &NodeAttachment,
    thrust: f32,
    body_pos: Vec2,
    body_rot: f32,
) -> Option<(Vec2, Vec2)> {
    let force = thruster.force(thrust, body_rot);
    if force == Vec2::ZERO {
        return None;
    }
    let point = body_pos + Vec2::from_angle(body_rot).rotate(attachment.offset.value());
    Some((force, point))
}

/// **The thruster pass:** one system over the thruster nodes. Each attached
/// thruster pushes its body with the driven thrust if a signal sets one, else
/// its authored thrust, along its axis, at its point. A free thruster, one
/// whose body is gone and one on a body that is not dynamic push nothing.
///
/// Cost: `O(T)` for `T` behavior nodes — per thruster one id lookup (a
/// hash probe) and one `get_mut` on the body query; no allocation.
#[expect(clippy::type_complexity, reason = "one filtered query")]
pub fn apply_thrusters(
    index: Res<IdIndex>,
    thrusters: Query<(&NodeKind, &NodeAttachment, Option<&SignalDrive>), With<BehaviorNode>>,
    mut bodies: Query<(Forces, &RigidBody), (With<Body>, Without<BehaviorNode>)>,
) {
    for (kind, attachment, drive) in &thrusters {
        let NodeKind::Thruster(thruster) = kind else {
            continue;
        };
        let Some(target) = attachment.target.and_then(|id| index.entity(id)) else {
            continue;
        };
        let Ok((mut forces, rigid_body)) = bodies.get_mut(target) else {
            continue;
        };
        if !rigid_body.is_dynamic() {
            continue;
        }
        let thrust = drive
            .and_then(|d| d.thrust)
            .unwrap_or_else(|| thruster.thrust.value());
        let Some((force, point)) = thruster_push(
            thruster,
            attachment,
            thrust,
            forces.position(),
            forces.rotation().as_radians(),
        ) else {
            continue;
        };
        forces.apply_force_at_point(force, point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gradiance_core::ids::StableId;
    use gradiance_domain::thruster::Thruster;
    use gradiance_units::{Angle, Force};

    #[test]
    fn a_thruster_pushes_at_its_point_in_the_body_frame() {
        let thruster = Thruster {
            thrust: Force(4.0),
            angle: Angle(0.0),
        };
        let attachment = NodeAttachment::to(StableId::new(), Vec2::new(0.0, 1.0));
        // The body is turned a quarter: its local +x is the world +y and its
        // local +y the world -x.
        let (force, point) = thruster_push(
            &thruster,
            &attachment,
            4.0,
            Vec2::new(2.0, 0.0),
            core::f32::consts::FRAC_PI_2,
        )
        .expect("a push");
        assert!((force - Vec2::new(0.0, 4.0)).length() < 1e-5, "{force}");
        assert!((point - Vec2::new(1.0, 0.0)).length() < 1e-5, "{point}");
    }

    #[test]
    fn no_thrust_is_no_push() {
        let attachment = NodeAttachment::to(StableId::new(), Vec2::ZERO);
        let t = Thruster::default();
        assert!(thruster_push(&t, &attachment, 0.0, Vec2::ZERO, 0.0).is_none());
        assert!(thruster_push(&t, &attachment, f32::NAN, Vec2::ZERO, 0.0).is_none());
    }
}
