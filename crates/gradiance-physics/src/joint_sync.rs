//! Authored `JointDef` → native engine joints, `Changed<>`-driven.
//!
//! The derived avian joint components live directly **on** the authored
//! joint entity (1:1, no orphan bookkeeping). World pins spawn a derived
//! static anchor body as a *child* of the joint entity — it has **no
//! collider**, so the classic pin-collision instability is
//! unrepresentable, and recursive despawn cleans it up for free.
//!
//! **Breaking limits.** Every derived joint carries avian's [`JointForces`]
//! (the post-solve reaction, read by `physics::queries::joint_force`). A joint
//! with an authored `break_force` is checked after each step by
//! [`break_overloaded_joints`], which sets the *authored* `broken` flag — a
//! simulation result like a pose, so the run boundary records it and undo
//! restores the joint. The sync then derives avian's [`JointDisabled`] from
//! that flag; `JointDisabled` itself is never saved.

use crate::drive::{SignalDrive, undriven_target};
use avian2d::prelude::*;
use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use gradiance_core::ids::{IdIndex, StableId};
use gradiance_domain::joint::{
    AngularMotorDef, JointDef, JointKind, LinearMotorDef, MIN_MOTOR_EFFORT, MOTOR_FORCE_PER_MASS,
    MOTOR_TORQUE_PER_INERTIA, exceeds_break_force, motor_ceiling,
};
use gradiance_domain::tuning;
use gradiance_units::Force;

/// Marker: this joint's referenced bodies were not all alive at last
/// sync; retried every frame until they are (makes spawn order — e.g.
/// scene loading — irrelevant).
#[derive(Component, Debug, Default)]
pub struct JointUnresolved;

/// Derived world-pin anchor entity for this joint, if any.
#[derive(Component, Debug)]
pub struct PinAnchor(pub Entity);

fn angular_motor(m: &AngularMotorDef) -> AngularMotor {
    // The ceiling and the gain depend on the bodies' mass and the substep
    // length; [`retune_joints`] sets both right after this insert, before the
    // next physics step.
    AngularMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity.value(),
        max_torque: MIN_MOTOR_EFFORT,
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: 0.0,
        },
        ..default()
    }
}

fn linear_motor(m: &LinearMotorDef) -> LinearMotor {
    LinearMotor {
        enabled: m.enabled,
        target_velocity: m.target_velocity.value(),
        max_force: MIN_MOTOR_EFFORT,
        motor_model: MotorModel::AccelerationBased {
            stiffness: 0.0,
            damping: 0.0,
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
            (resolved, def.anchor_b.value())
        } else {
            // World pin: derived static anchor at the world point, no
            // collider — nothing to collide, nothing to explode.
            let pin = commands
                .spawn((
                    RigidBody::Static,
                    Transform::from_translation(def.anchor_b.value().extend(0.0)),
                    ChildOf(entity),
                ))
                .id();
            commands.entity(entity).insert(PinAnchor(pin));
            (pin, Vec2::ZERO)
        };
        let mut entity_commands = commands.entity(entity);
        entity_commands.remove::<JointUnresolved>();
        // Broken → disabled, derived from the authored flag. Inserted *before*
        // the joint component so avian's on-add observer never puts a broken
        // joint into its joint graph, even for a frame.
        if def.common.broken {
            entity_commands.insert(JointDisabled);
        } else {
            entity_commands.remove::<JointDisabled>();
        }
        // The reaction-force readout (sensor + break check). Fresh on every
        // re-derive, so an edited joint never trips on a stale reading.
        entity_commands.insert(JointForces::new());

        // Rest orientation: body A's basis stays identity (so authored
        // axes are body-A local) and body B's basis absorbs the authored
        // relative angle. The constraint's rest state is then the
        // *creation-time* pose — welds hold it, sliders lock rotation to
        // it, hinge limits measure from it — instead of snapping rotated
        // bodies into alignment.
        //
        // A world-pin hinge or slider is derived world first, `(pin, body
        // A)`, so its motor, limits and angle or travel describe the body
        // against the world (`JointDef::world_first`); the basis is then the
        // opposite angle.
        let ends = if def.world_first() {
            [(body_b, anchor_b), (body_a, def.anchor_a.value())]
        } else {
            [(body_a, def.anchor_a.value()), (body_b, anchor_b)]
        };
        let basis_2 = if def.world_first() {
            (def.rest_rot_b - def.rest_rot_a).value()
        } else {
            (def.rest_rot_a - def.rest_rot_b).value()
        };
        insert_derived_joint(&mut entity_commands, def, ends, basis_2);

        if def.common.collide_connected {
            entity_commands.remove::<JointCollisionDisabled>();
        } else {
            entity_commands.insert(JointCollisionDisabled);
        }
    }
}

/// Inserts the engine joint (and, for a strut, its damping) for `def`'s kind.
/// `ends` are the engine's two bodies in order, each with its local anchor
/// (a world pin's derived anchor entity has `Vec2::ZERO`); `basis_2` is the
/// second body's rest orientation. The mass-derived values (a motor's
/// ceiling and gain, a spring's stiffness) are [`retune_joints`]'s.
fn insert_derived_joint(
    entity_commands: &mut EntityCommands,
    def: &JointDef,
    [(body_1, anchor_1), (body_2, anchor_2)]: [(Entity, Vec2); 2],
    basis_2: f32,
) {
    match &def.kind {
        JointKind::Hinge { limits, motor } => {
            let mut joint = RevoluteJoint::new(body_1, body_2)
                .with_local_anchor1(anchor_1)
                .with_local_anchor2(anchor_2)
                .with_local_basis2(basis_2);
            if let Some([min, max]) = limits {
                joint = joint.with_angle_limits(min.value(), max.value());
            }
            if let Some(m) = motor {
                joint = joint.with_motor(angular_motor(m));
            }
            entity_commands.insert(joint);
        }
        JointKind::Slider {
            axis,
            limits,
            motor,
        } => {
            // The axis is authored in body A's frame. Derived world first,
            // the first body is the pin (rotation 0), so the axis is carried
            // into the world at body A's rest angle — the direction the tool
            // drew (`JointDef::slider_frame`).
            let axis_1 = if def.world_first() {
                Vec2::from_angle((def.rest_rot_a - def.rest_rot_b).value()).rotate(*axis)
            } else {
                *axis
            };
            let mut joint = PrismaticJoint::new(body_1, body_2)
                .with_local_anchor1(anchor_1)
                .with_local_anchor2(anchor_2)
                .with_local_basis2(basis_2)
                .with_slider_axis(axis_1);
            if let Some([min, max]) = limits {
                joint = joint.with_limits(min.value(), max.value());
            }
            if let Some(m) = motor {
                joint = joint.with_motor(linear_motor(m));
            }
            entity_commands.insert(joint);
        }
        JointKind::Spring {
            rest_length,
            frequency,
            damping_ratio,
            range,
        } => {
            // An unbounded strut pins the distance limit to rest_length (a
            // pure spring), a ranged one uses the [min, max] band. The
            // compliance (1 / stiffness) comes from the reduced mass and is
            // set by `retune_joints`; rigid until then. The damping rate
            // carries no mass (avian applies it to the reduced mass).
            let (min, max) = match range {
                Some([lo, hi]) => (lo.value(), hi.value()),
                None => (rest_length.value(), rest_length.value()),
            };
            let joint = DistanceJoint::new(body_1, body_2)
                .with_local_anchor1(anchor_1)
                .with_local_anchor2(anchor_2)
                .with_limits(min, max)
                .with_compliance(0.0);
            entity_commands.insert((
                joint,
                JointDamping {
                    linear: tuning::spring_damping_rate(frequency.value(), *damping_ratio),
                    angular: 0.0,
                },
            ));
        }
    }
}

/// The reduced mass and angular inertia of a joint's two bodies (a static,
/// kinematic or not-yet-computed side counts as immovable).
fn reduced_of(
    [a, b]: [Entity; 2],
    bodies: &Query<(&RigidBody, &ComputedMass, &ComputedAngularInertia)>,
) -> (f32, f32) {
    let inv = |e: Entity| {
        bodies.get(e).map_or((0.0, 0.0), |(rb, m, i)| {
            let moves = rb.is_dynamic();
            (
                tuning::inverse(m.value(), moves),
                tuning::inverse(i.value(), moves),
            )
        })
    };
    let ((ma, ia), (mb, ib)) = (inv(a), inv(b));
    (tuning::reduced(ma, mb), tuning::reduced(ia, ib))
}

/// Re-derives every joint's **mass-derived** engine values from the bodies'
/// current mass and the substep length (scaled by the simulation speed): a motor's ceiling (auto: scaled to
/// the reduced inertia or mass) and its per-substep gain, a spring's
/// stiffness from its authored frequency. Runs every frame after
/// [`sync_joints`]; it writes only on a difference, so a density edit or a
/// resize retunes a joint without re-creating it (brief 4b-4).
///
/// It is also where a motor's **signal drive** lands (brief N-1): a
/// [`SignalDrive`] speed or cap replaces the authored one in the live engine
/// joint, and with no drive the authored speed is restored
/// ([`undriven_target`]), so unbinding needs no command.
#[expect(clippy::type_complexity, reason = "one optional engine joint per kind")]
pub fn retune_joints(
    mut joints: Query<(
        &JointDef,
        Option<&mut RevoluteJoint>,
        Option<&mut PrismaticJoint>,
        Option<&mut DistanceJoint>,
        Option<&SignalDrive>,
    )>,
    bodies: Query<(&RigidBody, &ComputedMass, &ComputedAngularInertia)>,
    fixed: Res<Time<Fixed>>,
    physics_time: Res<Time<Physics>>,
    substeps: Res<SubstepCount>,
) {
    // avian steps the fixed timestep scaled by the physics clock's relative
    // speed (`SimSettings::speed`), split into substeps.
    #[expect(clippy::cast_precision_loss, reason = "a small substep count")]
    let substep_secs =
        fixed.timestep().as_secs_f32() * physics_time.relative_speed() / substeps.0.max(1) as f32;
    for (def, revolute, prismatic, distance, drive) in &mut joints {
        let drive = drive.copied().unwrap_or_default();
        match &def.kind {
            JointKind::Hinge { motor: Some(m), .. } => {
                let Some(mut joint) = revolute else { continue };
                let (_, inertia) = reduced_of([joint.body1, joint.body2], &bodies);
                let ceiling = drive.motor_cap.map_or_else(
                    || motor_ceiling(m.max_torque.value(), inertia, MOTOR_TORQUE_PER_INERTIA),
                    |cap| cap.max(0.0),
                );
                let model = velocity_model(tuning::motor_gain(m.damping, substep_secs));
                let target = drive.motor_speed.unwrap_or_else(|| {
                    undriven_target(
                        m.target_velocity.value(),
                        m.oscillate,
                        joint.motor.target_velocity,
                    )
                });
                if differs(joint.motor.max_torque, ceiling) {
                    joint.motor.max_torque = ceiling;
                }
                if joint.motor.motor_model != model {
                    joint.motor.motor_model = model;
                }
                if differs(joint.motor.target_velocity, target) {
                    joint.motor.target_velocity = target;
                }
            }
            JointKind::Slider { motor: Some(m), .. } => {
                let Some(mut joint) = prismatic else { continue };
                let (mass, _) = reduced_of([joint.body1, joint.body2], &bodies);
                let ceiling = drive.motor_cap.map_or_else(
                    || motor_ceiling(m.max_force.value(), mass, MOTOR_FORCE_PER_MASS),
                    |cap| cap.max(0.0),
                );
                let model = velocity_model(tuning::motor_gain(m.damping, substep_secs));
                let target = drive.motor_speed.unwrap_or_else(|| {
                    undriven_target(
                        m.target_velocity.value(),
                        m.oscillate,
                        joint.motor.target_velocity,
                    )
                });
                if differs(joint.motor.max_force, ceiling) {
                    joint.motor.max_force = ceiling;
                }
                if joint.motor.motor_model != model {
                    joint.motor.motor_model = model;
                }
                if differs(joint.motor.target_velocity, target) {
                    joint.motor.target_velocity = target;
                }
            }
            JointKind::Spring { frequency, .. } => {
                let Some(mut joint) = distance else { continue };
                let (mass, _) = reduced_of([joint.body1, joint.body2], &bodies);
                let stiffness = tuning::spring_stiffness(frequency.value(), mass);
                let compliance = if stiffness > 0.0 {
                    1.0 / stiffness
                } else {
                    0.0
                };
                if differs(joint.compliance, compliance) {
                    joint.compliance = compliance;
                }
            }
            _ => {}
        }
    }
}

/// The velocity-tracking motor model avian takes, at per-substep `gain`.
fn velocity_model(gain: f32) -> MotorModel {
    MotorModel::AccelerationBased {
        stiffness: 0.0,
        damping: gain,
    }
}

/// Whether `new` differs from `old` beyond rounding.
fn differs(old: f32, new: f32) -> bool {
    (old - new).abs() > f32::EPSILON * old.abs().max(new.abs()).max(1.0)
}

/// Breaks every intact joint whose linear reaction force exceeds its authored
/// `break_force` — by writing the **authored** `broken` flag, the same way the
/// solver writes poses during a run (invariants 1–2 govern *editing*; this is
/// simulation). [`sync_joints`] then derives [`JointDisabled`] from it. Runs
/// after each physics step, only while the simulation plays.
pub fn break_overloaded_joints(mut joints: Query<(&mut JointDef, &JointForces)>) {
    for (mut def, forces) in &mut joints {
        if def.common.broken {
            continue;
        }
        let force = Force(forces.force().length());
        if exceeds_break_force(def.common.break_force, force) {
            debug!(force = force.value(), "joint broke");
            def.common.broken = true;
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
