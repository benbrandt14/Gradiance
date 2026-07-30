//! Authored `JointDef` → native engine joints, `Changed<>`-driven.
//!
//! # Where the derived joint lives
//!
//! rapier resolves an `ImpulseJoint`'s second endpoint by walking `ChildOf`
//! upward from the entity that holds it, so the component must sit on (a
//! descendant of) body B. The authored joint entity keeps its `StableId` and
//! `JointDef` and stays free-standing; a small **derived** entity is spawned as
//! a child of body B to carry the engine joint, linked back by [`DerivedJoint`].
//!
//! Despawning body B therefore cleans the joint up for free, and the authored
//! record — the thing identity and persistence care about — never moves.
//!
//! # Why all three kinds are `GenericJoint`
//!
//! Only `GenericJointBuilder` exposes `local_basis1`/`local_basis2`, and those
//! carry the authored *rest orientation*: the constraint's rest state is the
//! creation-time pose, so sliders lock rotation to it and hinge limits measure
//! from it, instead of snapping rotated bodies into alignment. The typed
//! builders drop that, so the generic form is the only one that preserves
//! existing behaviour.
//!
//! World pins spawn a derived static anchor body — with **no collider**, so the
//! classic pin-collision instability is unrepresentable.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_core::units::PlaneFrame;
use gradiance_domain::joint::{AngularMotorDef, JointDef, JointKind, LinearMotorDef};

/// Marker: this joint's referenced bodies were not all alive at last sync;
/// retried every frame until they are (so spawn order — a scene load, say — is
/// irrelevant).
#[derive(Component, Debug, Default)]
pub struct JointUnresolved;

/// Derived world-pin anchor entity for this joint, if any.
#[derive(Component, Debug)]
pub struct PinAnchor(pub Entity);

/// The derived engine-joint entity for an authored joint.
///
/// It is a child of body B (see the module docs), so it cannot live on the
/// authored entity itself; this is the link back.
#[derive(Component, Debug)]
pub struct DerivedJoint(pub Entity);

/// (Re)derives engine joints for authored joints that changed or are awaiting
/// body resolution.
pub fn sync_joints(
    mut commands: Commands,
    index: Res<IdIndex>,
    changed: Query<
        (Entity, &JointDef, Option<&PinAnchor>, Option<&DerivedJoint>),
        Or<(Changed<JointDef>, With<JointUnresolved>)>,
    >,
    masses: Query<&ReadMassProperties>,
) {
    let plane = PlaneFrame::XY;
    for (entity, def, old_pin, old_joint) in &changed {
        // Drop any previously derived state (the kind may have changed).
        clear_derived(&mut commands, entity, old_pin, old_joint);

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
            // World pin: a derived static anchor at the world point, with no
            // collider — nothing to collide, nothing to explode.
            let pin = commands
                .spawn((
                    RigidBody::Fixed,
                    Transform::from_translation(plane.point(def.anchor_b, 0.0)),
                    ChildOf(entity),
                ))
                .id();
            commands.entity(entity).insert(PinAnchor(pin));
            (pin, Vec2::ZERO)
        };

        // An **auto** motor (`max_force <= 0`) sizes its ceiling from the driven
        // body's computed mass properties. The engine fills those a frame or two
        // after the body spawns, so if they are not ready yet, retry — exactly
        // like an unresolved endpoint — rather than bake in the floor.
        let auto_motor = match &def.kind {
            JointKind::Hinge { motor: Some(m), .. } => m.max_torque.value() <= 0.0,
            JointKind::Slider { motor: Some(m), .. } => m.max_force.value() <= 0.0,
            _ => false,
        };
        if auto_motor && masses.get(body_a).is_err() {
            commands.entity(entity).insert(JointUnresolved);
            continue;
        }
        let properties = masses.get(body_a).map(ReadMassProperties::get).ok();
        let mass = properties.map_or(0.0, |m| m.mass);
        let inertia = properties.map_or(0.0, |m| plane.unspin(m.principal_inertia).abs());

        // Rest orientation: body A's basis stays identity (so authored axes are
        // body-A local) and body B's basis absorbs the authored relative angle.
        let basis_b = def.rest_rot_a - def.rest_rot_b;
        let mut joint = generic_joint_for(def, &plane, anchor_b, basis_b, inertia, mass);
        joint.set_contacts_enabled(def.common.collide_connected);

        let derived = commands
            .spawn((
                ImpulseJoint::new(body_a, TypedJoint::GenericJoint(joint)),
                ChildOf(body_b),
            ))
            .id();
        commands
            .entity(entity)
            .remove::<JointUnresolved>()
            .insert(DerivedJoint(derived));
    }
}

/// Tears down whatever this authored joint derived last time.
fn clear_derived(
    commands: &mut Commands,
    entity: Entity,
    pin: Option<&PinAnchor>,
    joint: Option<&DerivedJoint>,
) {
    if let Some(joint) = joint {
        commands.entity(entity).remove::<DerivedJoint>();
        commands.entity(joint.0).try_despawn();
    }
    if let Some(pin) = pin {
        commands.entity(entity).remove::<PinAnchor>();
        commands.entity(pin.0).try_despawn();
    }
}

/// The degrees of freedom a hinge locks, given the plane already holds the body.
///
/// **Not** `LOCKED_REVOLUTE_AXES`. That mask locks all three translations plus
/// two rotations, but the simulation-plane constraint has already removed the
/// out-of-plane freedoms by *zeroing their inverse mass and inertia*. Asking
/// the solver to also constrain axes with no inverse inertia is asking it to
/// solve a degenerate system, and the error bleeds into the free axis — a
/// hinged arm swings an order of magnitude too slowly.
///
/// So the joint constrains only what the plane does not: the two in-plane
/// translations. This mirrors rapier's own 2D `LOCKED_REVOLUTE_AXES`, which is
/// exactly `LIN_X | LIN_Y` — the coplanar world is a 2D world, and its joints
/// should ask for 2D constraints.
fn hinge_locked_axes() -> JointAxesMask {
    JointAxesMask::LIN_X | JointAxesMask::LIN_Y
}

/// The degrees of freedom a slider locks, given the plane already holds the
/// body. See [`hinge_locked_axes`] for why this is not the stock 3D mask.
///
/// The joint frame's X is the slider axis, so the perpendicular in-plane
/// translation is `LIN_Y`; in-plane rotation is about the plane normal, which
/// that frame leaves as `ANG_Z`.
fn slider_locked_axes() -> JointAxesMask {
    JointAxesMask::LIN_Y | JointAxesMask::ANG_Z
}

/// The orthonormal joint frame whose local +X is `axis`.
///
/// rapier measures a revolute joint's free rotation as `AngX` and a prismatic
/// joint's free translation as `LinX`, both about the frame's X — so a world
/// axis is expressed by rotating the frame, not by naming a different degree of
/// freedom.
fn joint_frame(axis: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::X, axis.normalize_or(Vec3::X))
}

/// Builds the engine joint for `def`'s kind.
///
/// `anchor_b` is body-B local (or `Vec2::ZERO` for a world pin); `basis_b`
/// carries the authored rest orientation; `inertia`/`mass` are body A's
/// computed mass properties, used to size an auto motor's ceiling.
fn generic_joint_for(
    def: &JointDef,
    plane: &PlaneFrame,
    anchor_b: Vec2,
    basis_b: f32,
    inertia: f32,
    mass: f32,
) -> GenericJoint {
    let anchor1 = plane.dir(def.anchor_a);
    let anchor2 = plane.dir(anchor_b);

    match &def.kind {
        JointKind::Hinge { limits, motor } => {
            // The hinge turns about the plane normal — the one axis a 2D world
            // ever had.
            let frame = joint_frame(plane.normal());
            let mut builder = GenericJointBuilder::new(hinge_locked_axes())
                .local_anchor1(anchor1)
                .local_anchor2(anchor2)
                .local_basis1(frame)
                .local_basis2(frame * Quat::from_rotation_x(basis_b));
            if let Some([min, max]) = limits {
                builder = builder.limits(JointAxis::AngX, [*min, *max]);
            }
            if let Some(m) = motor {
                builder = angular_motor(&builder, m, inertia);
            }
            builder.build()
        }
        JointKind::Slider {
            axis,
            limits,
            motor,
        } => {
            let frame = joint_frame(plane.dir(*axis));
            let mut builder = GenericJointBuilder::new(slider_locked_axes())
                .local_anchor1(anchor1)
                .local_anchor2(anchor2)
                .local_basis1(frame)
                .local_basis2(frame * Quat::from_rotation_x(basis_b));
            if let Some([min, max]) = limits {
                builder = builder.limits(JointAxis::LinX, [*min, *max]);
            }
            if let Some(m) = motor {
                builder = linear_motor(&builder, m, mass);
            }
            builder.build()
        }
        JointKind::Spring {
            rest_length,
            stiffness,
            damping,
            range,
        } => {
            // A strut is a spring between two anchors, so it maps onto the
            // engine's own spring joint rather than a hand-built generic one:
            // the constraint acts along the *current* separation, which is what
            // makes a swinging strut behave like a strut instead of a rail.
            //
            // The 2D engine expressed this as a distance joint plus a separate
            // damping component; here stiffness and damping are the spring's own
            // terms, force-based so they read as a real spring-mass-damper.
            let mut joint = SpringJointBuilder::new(*rest_length, *stiffness, *damping)
                .local_anchor1(anchor1)
                .local_anchor2(anchor2)
                .spring_model(MotorModel::ForceBased)
                .build();
            if let Some([min, max]) = range {
                joint.data.set_limits(JointAxis::LinX, [*min, *max]);
            }
            joint.data
        }
    }
}

/// Applies an authored angular motor to the hinge's free axis.
fn angular_motor(
    builder: &GenericJointBuilder,
    m: &AngularMotorDef,
    inertia: f32,
) -> GenericJointBuilder {
    if !m.enabled {
        return *builder;
    }
    let ceiling = gradiance_domain::joint::motor_ceiling(
        m.max_torque.value(),
        inertia,
        gradiance_domain::joint::MOTOR_TORQUE_PER_INERTIA,
    );
    (*builder)
        .motor_model(JointAxis::AngX, MotorModel::AccelerationBased)
        .motor_velocity(JointAxis::AngX, m.target_velocity.value(), m.damping)
        .motor_max_force(JointAxis::AngX, ceiling)
}

/// Applies an authored linear motor to the slider's free axis.
fn linear_motor(
    builder: &GenericJointBuilder,
    m: &LinearMotorDef,
    mass: f32,
) -> GenericJointBuilder {
    if !m.enabled {
        return *builder;
    }
    let ceiling = gradiance_domain::joint::motor_ceiling(
        m.max_force.value(),
        mass,
        gradiance_domain::joint::MOTOR_FORCE_PER_MASS,
    );
    (*builder)
        .motor_model(JointAxis::LinX, MotorModel::AccelerationBased)
        .motor_velocity(JointAxis::LinX, m.target_velocity.value(), m.damping)
        .motor_max_force(JointAxis::LinX, ceiling)
}

/// Safety net: if a referenced body vanished outside the command path, tear
/// down the derived joint and mark it unresolved (it re-derives if the body
/// comes back).
pub fn guard_dangling_joints(
    mut commands: Commands,
    index: Res<IdIndex>,
    joints: Query<
        (Entity, &JointDef, Option<&PinAnchor>, Option<&DerivedJoint>),
        Without<JointUnresolved>,
    >,
) {
    for (entity, def, pin, derived) in &joints {
        let dangling = def
            .referenced_bodies()
            .any(|id: StableId| index.entity(id).is_none());
        if dangling {
            warn!(?entity, "joint lost a referenced body; disabling");
            clear_derived(&mut commands, entity, pin, derived);
            commands.entity(entity).insert(JointUnresolved);
        }
    }
}
