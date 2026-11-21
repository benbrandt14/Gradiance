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
    bodies: Query<&RigidBody>,
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
        // Broken → disabled, derived from the authored flag. A joint between
        // two immovable ends is derived disabled too: it solves nothing (two
        // statics cannot move each other), and avian's islands cannot link
        // it — a merge of two bodies that are in no island would panic.
        // The authored joint stays (sensors read angle and length from the
        // bodies' poses; undo and persistence are untouched).
        // Inserted *before* the joint component so avian's on-add observer
        // never puts such a joint into its joint graph, even for a frame.
        let movable = |body: Entity| {
            bodies
                .get(body)
                .is_ok_and(|rb| rb.is_dynamic() || rb.is_kinematic())
        };
        if def.common.broken || (!movable(body_a) && !movable(body_b)) {
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
        // An authored joint change (creation, edit, undo/redo) must reach
        // bodies that may be asleep — avian does not wake on joint changes.
        // This is also the old island-panic path (welding touching boxes
        // inserts `JointCollisionDisabled`), now exercised every time.
        for body in [body_a, body_b] {
            if let Ok(kind) = bodies.get(body)
                && (kind.is_dynamic() || kind.is_kinematic())
            {
                commands.queue(WakeBody(body));
            }
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
fn reduced_of([a, b]: [Entity; 2], bodies: &JointBodies) -> (f32, f32) {
    let inv = |e: Entity| {
        bodies.get(e).map_or((0.0, 0.0), |(rb, m, i, _)| {
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

/// The per-body state [`retune_joints`] reads: what a body is, its computed
/// mass properties, and whether a motor has parked [`SleepingDisabled`] on it.
type JointBodies<'w, 's> = Query<
    'w,
    's,
    (
        &'static RigidBody,
        &'static ComputedMass,
        &'static ComputedAngularInertia,
        Has<SleepingDisabled>,
    ),
>;
/// Parks or lifts the motor's **no-sleep guard** ([`SleepingDisabled`]) on a
/// joint's bodies: an enabled motor keeps them awake — `Box2D`'s rule — so a
/// motorized hinge keeps turning and a driven slider keeps sliding however
/// long the bodies would otherwise rest. Only the moving ends carry the
/// guard; a static or world-pin end never sleeps to begin with.
fn apply_motor_sleep_guard(
    commands: &mut Commands,
    bodies: &JointBodies,
    a: Entity,
    b: Entity,
    motored: bool,
) {
    for entity in [a, b] {
        let Ok((rb, _, _, guarded)) = bodies.get(entity) else {
            continue;
        };
        if !(rb.is_dynamic() || rb.is_kinematic()) || motored == guarded {
            continue;
        }
        if motored {
            commands.entity(entity).insert(SleepingDisabled);
        } else {
            commands.entity(entity).remove::<SleepingDisabled>();
        }
    }
}

/// A joint motor's retuned values: the mass-derived ceiling, the gain model,
/// the target velocity, and whether the motor is enabled (a driven
/// non-positive cap switches it off — avian reads such a cap as
/// "unlimited"). Pure.
fn motor_tuning(
    authored_ceiling: f32,
    reduced: f32,
    per_unit: f32,
    damping: f32,
    substep_secs: f32,
    drive: &SignalDrive,
    authored: &impl MotorParams,
    live: f32,
) -> (f32, MotorModel, f32, bool) {
    let ceiling = drive
        .motor_cap
        .unwrap_or_else(|| motor_ceiling(authored_ceiling, reduced, per_unit));
    let model = velocity_model(tuning::motor_gain(damping, substep_secs));
    let target = drive
        .motor_speed
        .unwrap_or_else(|| undriven_target(authored.target(), authored.oscillates(), live));
    (
        ceiling,
        model,
        target,
        authored.enabled() && drive.motor_cap.is_none_or(|cap| cap > 0.0),
    )
}

/// The authored values [`motor_tuning`] reads from either motor kind.
trait MotorParams {
    /// The authored target speed (rad/s or m/s).
    fn target(&self) -> f32;
    /// Whether the authored motor oscillates.
    fn oscillates(&self) -> bool;
    /// Whether the authored motor is enabled.
    fn enabled(&self) -> bool;
}

impl MotorParams for AngularMotorDef {
    fn target(&self) -> f32 {
        self.target_velocity.value()
    }
    fn oscillates(&self) -> bool {
        self.oscillate
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
}

impl MotorParams for LinearMotorDef {
    fn target(&self) -> f32 {
        self.target_velocity.value()
    }
    fn oscillates(&self) -> bool {
        self.oscillate
    }
    fn enabled(&self) -> bool {
        self.enabled
    }
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
    mut commands: Commands,
    mut joints: Query<(
        &JointDef,
        Option<&mut RevoluteJoint>,
        Option<&mut PrismaticJoint>,
        Option<&mut DistanceJoint>,
        Option<&SignalDrive>,
    )>,
    bodies: JointBodies,
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
                let (ceiling, model, target, enabled) = motor_tuning(
                    m.max_torque.value(),
                    inertia,
                    MOTOR_TORQUE_PER_INERTIA,
                    m.damping,
                    substep_secs,
                    &drive,
                    m,
                    joint.motor.target_velocity,
                );
                if differs(joint.motor.max_torque, ceiling) {
                    joint.motor.max_torque = ceiling;
                }
                if joint.motor.enabled != enabled {
                    joint.motor.enabled = enabled;
                }
                if joint.motor.motor_model != model {
                    joint.motor.motor_model = model;
                }
                if differs(joint.motor.target_velocity, target) {
                    joint.motor.target_velocity = target;
                }
                apply_motor_sleep_guard(&mut commands, &bodies, joint.body1, joint.body2, enabled);
            }
            JointKind::Slider { motor: Some(m), .. } => {
                let Some(mut joint) = prismatic else { continue };
                let (mass, _) = reduced_of([joint.body1, joint.body2], &bodies);
                let (ceiling, model, target, enabled) = motor_tuning(
                    m.max_force.value(),
                    mass,
                    MOTOR_FORCE_PER_MASS,
                    m.damping,
                    substep_secs,
                    &drive,
                    m,
                    joint.motor.target_velocity,
                );
                if differs(joint.motor.max_force, ceiling) {
                    joint.motor.max_force = ceiling;
                }
                if joint.motor.enabled != enabled {
                    joint.motor.enabled = enabled;
                }
                if joint.motor.motor_model != model {
                    joint.motor.motor_model = model;
                }
                if differs(joint.motor.target_velocity, target) {
                    joint.motor.target_velocity = target;
                }
                apply_motor_sleep_guard(&mut commands, &bodies, joint.body1, joint.body2, enabled);
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
                // A spring holds, it does not drive: no guard here, and one
                // left by an earlier motorized kind must come off.
                apply_motor_sleep_guard(&mut commands, &bodies, joint.body1, joint.body2, false);
            }
            JointKind::Hinge { .. } | JointKind::Slider { .. } => {
                // No motor on this joint: a guard left by an earlier
                // motorized kind must come off so the bodies may sleep.
                let ends = revolute
                    .as_deref()
                    .map(|j| (j.body1, j.body2))
                    .or_else(|| prismatic.as_deref().map(|j| (j.body1, j.body2)))
                    .or_else(|| distance.as_deref().map(|j| (j.body1, j.body2)));
                if let Some((a, b)) = ends {
                    apply_motor_sleep_guard(&mut commands, &bodies, a, b, false);
                }
            }
        }
    }
}

/// Moves each body-to-body slider's **engine** anchor on body 1 along the
/// slide axis to where body 2's anchor projects onto it, and shifts the
/// travel limits by the same amount, before every physics step.
///
/// The constraint is unchanged: the slide line is the same line, and the
/// limits bound the same travel. What changes is the lever arm. avian's
/// prismatic applies its sideways correction at the two *anchors*; with
/// body 2 far along the axis, body 1 got that push at its own anchor (near
/// its centre) while the real contact is under body 2, a wrong lever that
/// grows with the travel. A cart run off the end of a light, pinned rail
/// reached thousands of m/s at about 4 m of travel, at any substep count
/// (joint stress test, 2026-11-20). With the anchor under body 2 the lever
/// is the true one (as `Box2D`'s prismatic does).
///
/// A world-pin slider is left alone: its first body is the static pin,
/// which takes no correction. Authored state is not touched; the next
/// [`sync_joints`] re-derive starts from the authored anchor again.
pub fn track_slider_anchors(
    mut joints: Query<(&JointDef, &mut PrismaticJoint)>,
    poses: Query<(&Position, &Rotation)>,
) {
    for (def, mut joint) in &mut joints {
        let JointKind::Slider { limits, .. } = def.kind else {
            continue;
        };
        if def.world_first() {
            continue;
        }
        let (Ok((p1, r1)), Ok((p2, r2))) = (poses.get(joint.body1), poses.get(joint.body2)) else {
            continue;
        };
        let authored = def.anchor_a.value();
        let axis = joint.slider_axis.normalize_or_zero();
        let on_2 = p2.0 + *r2 * def.anchor_b.value();
        let local = r1.inverse() * (on_2 - p1.0);
        let shift = (local - authored).dot(axis);
        if !shift.is_finite() {
            continue;
        }
        let anchor = authored + axis * shift;
        let moved = match joint.frame1.anchor {
            JointAnchor::Local(old) => old.distance_squared(anchor) > 1e-12,
            JointAnchor::FromGlobal(_) => true,
        };
        if moved {
            joint.frame1.anchor = JointAnchor::Local(anchor);
            joint.limits = limits.map(|[min, max]| DistanceLimit {
                min: min.value() - shift,
                max: max.value() - shift,
            });
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
