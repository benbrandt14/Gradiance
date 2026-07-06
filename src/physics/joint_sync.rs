//! Authored `JointDef` → native engine joints, `Changed<>`-driven.
//!
//! The derived avian joint components live directly **on** the authored
//! joint entity (1:1, no orphan bookkeeping). World pins spawn a derived
//! static anchor body as a *child* of the joint entity — it has **no
//! collider**, so the classic pin-collision instability is
//! unrepresentable, and recursive despawn cleans it up for free.

use crate::core::ids::{IdIndex, StableId};
use crate::domain::joint::{JointCommon, JointDef, JointKind, MotorDef};
use avian2d::math::Vector;
use avian2d::prelude::*;
use bevy::prelude::*;

/// Marker: this joint's referenced bodies were not all alive at last
/// sync; retried every frame until they are (makes spawn order — e.g.
/// scene loading — irrelevant).
#[derive(Component, Debug, Default)]
pub struct JointUnresolved;

/// Derived world-pin anchor entity for this joint, if any.
#[derive(Component, Debug)]
pub struct PinAnchor(pub Entity);

fn angular_motor(m: &MotorDef) -> AngularMotor {
    AngularMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity,
        max_torque: m.max_force,
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: m.damping,
        },
        ..default()
    }
}

fn linear_motor(m: &MotorDef) -> LinearMotor {
    LinearMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity,
        max_force: m.max_force,
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: m.damping,
        },
        ..default()
    }
}

/// (Re)derives engine joints for authored joints that changed or are
/// awaiting body resolution.
pub fn sync_joints(
    mut commands: Commands,
    index: Res<IdIndex>,
    changed: Query<
        (Entity, &JointDef, Option<&PinAnchor>),
        Or<(Changed<JointDef>, With<JointUnresolved>)>,
    >,
) {
    for (entity, def, old_pin) in &changed {
        // Drop any previously derived state (kind may have changed).
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<(RevoluteJoint, FixedJoint, PrismaticJoint)>();
        if let Some(pin) = old_pin {
            entity_commands.remove::<PinAnchor>();
            commands.entity(pin.0).despawn();
        }

        // Resolve endpoints; retry next frame if any is missing.
        let Some(body_a) = index.entity(def.body_a) else {
            commands.entity(entity).insert(JointUnresolved);
            continue;
        };
        let (body_b, anchor_b) = if let Some(id) = def.body_b {
            let Some(resolved) = index.entity(id) else {
                commands.entity(entity).insert(JointUnresolved);
                continue;
            };
            (resolved, def.anchor_b)
        } else {
            // World pin: derived static anchor at the world point, no
            // collider — nothing to collide, nothing to explode.
            let pin = commands
                .spawn((
                    RigidBody::Static,
                    Transform::from_translation(def.anchor_b.extend(0.0)),
                    ChildOf(entity),
                ))
                .id();
            commands.entity(entity).insert(PinAnchor(pin));
            (pin, Vec2::ZERO)
        };
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<JointUnresolved>();

        // Rest orientation: body A's basis stays identity (so authored
        // axes are body-A local) and body B's basis absorbs the authored
        // relative angle. The constraint's rest state is then the
        // *creation-time* pose — welds hold it, sliders lock rotation to
        // it, hinge limits measure from it — instead of snapping rotated
        // bodies into alignment.
        let basis_b = def.rest_rot_a - def.rest_rot_b;
        match &def.kind {
            JointKind::Hinge { limits, motor } => {
                let mut joint = RevoluteJoint::new(body_a, body_b)
                    .with_local_anchor1(Vector::from(def.anchor_a))
                    .with_local_anchor2(Vector::from(anchor_b))
                    .with_local_basis2(basis_b);
                if let Some([min, max]) = limits {
                    joint = joint.with_angle_limits(*min, *max);
                }
                if let Some(m) = motor {
                    joint = joint.with_motor(angular_motor(m));
                }
                entity_commands.insert(joint);
            }
            JointKind::Weld => {
                let joint = FixedJoint::new(body_a, body_b)
                    .with_local_anchor1(Vector::from(def.anchor_a))
                    .with_local_anchor2(Vector::from(anchor_b))
                    .with_local_basis2(basis_b);
                entity_commands.insert(joint);
            }
            JointKind::Slider {
                axis,
                limits,
                motor,
            } => {
                let mut joint = PrismaticJoint::new(body_a, body_b)
                    .with_local_anchor1(Vector::from(def.anchor_a))
                    .with_local_anchor2(Vector::from(anchor_b))
                    .with_local_basis2(basis_b)
                    .with_slider_axis(Vector::from(*axis));
                if let Some([min, max]) = limits {
                    joint = joint.with_limits(*min, *max);
                }
                if let Some(m) = motor {
                    joint = joint.with_motor(linear_motor(m));
                }
                entity_commands.insert(joint);
            }
        }

        if def.common.collide_connected {
            entity_commands.remove::<JointCollisionDisabled>();
        } else {
            entity_commands.insert(JointCollisionDisabled);
        }
        let _ = JointCommon::default(); // referenced for doc-link stability
    }
}

/// Safety net: if a referenced body vanished outside the command path,
/// tear down the derived joint and mark it unresolved (it re-derives if
/// the body comes back).
pub fn guard_dangling_joints(
    mut commands: Commands,
    index: Res<IdIndex>,
    joints: Query<(Entity, &JointDef), Without<JointUnresolved>>,
) {
    for (entity, def) in &joints {
        let dangling = def
            .referenced_bodies()
            .any(|id: StableId| index.entity(id).is_none());
        if dangling {
            warn!(?entity, "joint lost a referenced body; disabling");
            commands
                .entity(entity)
                .remove::<(RevoluteJoint, FixedJoint, PrismaticJoint)>()
                .insert(JointUnresolved);
        }
    }
}
