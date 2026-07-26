//! Authored `JointDef` → native engine joints, `Changed<>`-driven.
//!
//! The derived avian joint components live directly **on** the authored
//! joint entity (1:1, no orphan bookkeeping). World pins spawn a derived
//! static anchor body as a *child* of the joint entity — it has **no
//! collider**, so the classic pin-collision instability is
//! unrepresentable, and recursive despawn cleans it up for free.

use avian2d::math::Vector;
use avian2d::prelude::*;
use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::joint::{AngularMotorDef, JointDef, JointKind, LinearMotorDef};

/// Marker: this joint's referenced bodies were not all alive at last
/// sync; retried every frame until they are (makes spawn order — e.g.
/// scene loading — irrelevant).
#[derive(Component, Debug, Default)]
pub struct JointUnresolved;

/// Derived world-pin anchor entity for this joint, if any.
#[derive(Component, Debug)]
pub struct PinAnchor(pub Entity);

fn angular_motor(m: &AngularMotorDef, inertia: f32) -> AngularMotor {
    AngularMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity.value(),
        // Auto (`max_torque <= 0`) scales with the driven body's real angular
        // inertia; an explicit authored cap is used as-is.
        max_torque: gradiance_domain::joint::motor_ceiling(
            m.max_torque.value(),
            inertia,
            gradiance_domain::joint::MOTOR_TORQUE_PER_INERTIA,
        ),
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: m.damping,
        },
        ..default()
    }
}

fn linear_motor(m: &LinearMotorDef, mass: f32) -> LinearMotor {
    LinearMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity.value(),
        max_force: gradiance_domain::joint::motor_ceiling(
            m.max_force.value(),
            mass,
            gradiance_domain::joint::MOTOR_FORCE_PER_MASS,
        ),
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
    masses: Query<&ComputedMass>,
    inertias: Query<&ComputedAngularInertia>,
) {
    for (entity, def, old_pin) in &changed {
        // Drop any previously derived state (kind may have changed).
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<(RevoluteJoint, PrismaticJoint, DistanceJoint, JointDamping)>();
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
        // An **auto** motor (`max_force <= 0`) scales its ceiling with the
        // driven body's computed mass properties. avian fills those in a frame
        // or two after the body spawns, so if they aren't ready yet, retry —
        // exactly like an unresolved endpoint — rather than bake the floor.
        let auto_motor = match &def.kind {
            JointKind::Hinge { motor: Some(m), .. } => m.max_torque.value() <= 0.0,
            JointKind::Slider { motor: Some(m), .. } => m.max_force.value() <= 0.0,
            _ => false,
        };
        if auto_motor && inertias.get(body_a).is_err() {
            commands.entity(entity).insert(JointUnresolved);
            continue;
        }
        let inertia = inertias.get(body_a).map_or(0.0, |i| i.value());
        let mass = masses.get(body_a).map_or(0.0, |m| m.value());

        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<JointUnresolved>();

        // Rest orientation: body A's basis stays identity (so authored
        // axes are body-A local) and body B's basis absorbs the authored
        // relative angle. The constraint's rest state is then the
        // *creation-time* pose — welds hold it, sliders lock rotation to
        // it, hinge limits measure from it — instead of snapping rotated
        // bodies into alignment.
        let basis_b = def.rest_rot_a - def.rest_rot_b;
        insert_derived_joint(
            &mut entity_commands,
            def,
            body_a,
            body_b,
            anchor_b,
            basis_b,
            inertia,
            mass,
        );

        if def.common.collide_connected {
            entity_commands.remove::<JointCollisionDisabled>();
        } else {
            entity_commands.insert(JointCollisionDisabled);
        }
    }
}

/// Inserts the engine joint (and, for a strut, its damping) for `def`'s kind.
/// `anchor_b` is body-B local (or `Vec2::ZERO` for a world pin); `basis_b`
/// carries the authored rest orientation; `inertia`/`mass` are body A's
/// computed mass properties, used to size an auto motor's ceiling.
#[expect(clippy::too_many_arguments, reason = "one value per derived field")]
fn insert_derived_joint(
    entity_commands: &mut EntityCommands,
    def: &JointDef,
    body_a: Entity,
    body_b: Entity,
    anchor_b: Vec2,
    basis_b: f32,
    inertia: f32,
    mass: f32,
) {
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
                joint = joint.with_motor(angular_motor(m, inertia));
            }
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
                joint = joint.with_motor(linear_motor(m, mass));
            }
            entity_commands.insert(joint);
        }
        JointKind::Spring {
            rest_length,
            stiffness,
            damping,
            range,
        } => {
            // compliance = 1 / stiffness (0 = rigid); an unbounded strut pins
            // the distance limit to rest_length (a pure spring), a ranged one
            // uses the [min, max] band. Damping is a separate joint component.
            let compliance = if *stiffness > 0.0 {
                1.0 / stiffness
            } else {
                0.0
            };
            let (min, max) = match range {
                Some([lo, hi]) => (*lo, *hi),
                None => (*rest_length, *rest_length),
            };
            let joint = DistanceJoint::new(body_a, body_b)
                .with_local_anchor1(Vector::from(def.anchor_a))
                .with_local_anchor2(Vector::from(anchor_b))
                .with_limits(min, max)
                .with_compliance(compliance);
            entity_commands.insert((
                joint,
                JointDamping {
                    linear: *damping,
                    angular: 0.0,
                },
            ));
        }
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
                .remove::<(RevoluteJoint, PrismaticJoint, DistanceJoint, JointDamping)>()
                .insert(JointUnresolved);
        }
    }
}
