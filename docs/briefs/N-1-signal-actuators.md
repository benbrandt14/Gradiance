# Signals that drive physics
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A signal binding can drive a physical quantity, not only a colour. New
sinks: hinge/slider **motor speed**, **motor torque/force cap**, **field
strength**, **gravity scale**. The node graph then builds pistons,
oscillators and feedback loops (a `defsignal` over a sensor into a motor).

## Facts (planning thread, 2026-11-19, on `92ab302`)

- Every sink is visual today: `SignalSink::{Fill, TracerColor, Plot}`
  (`gradiance-domain/src/signal.rs` ≈ line 252).
- Visual sinks write a **derived** `SignalColorOverride`; authored
  appearance stays untouched. Actuator sinks follow the same pattern: a
  derived override that `physics` reads, never an authored write, never
  an undo step (invariant 5).
- Motors are `AngularMotorDef` / `LinearMotorDef` on `JointKind`;
  `physics::joint_sync` builds `MotorModel::AccelerationBased`
  (≈ line 327). Fields superpose through `Fields::accel_at`.
- Signals run in the Tier-B kernel (`gradiance-signal`, `kernel`); the VM
  must not run per frame.

## Decisions (best estimate)

1. One derived `SignalDrive` component per target, keyed by `StableId`,
   holding only the overridden fields. Unbinding removes it, so the
   authored value comes back with no command.
2. The node graph shows the new sinks as actuator pins; units come from
   the quantity catalog (5-2).

## Out of scope

Thrusters (N-3); key inputs (N-2).

## Done when

- A sine `defsignal` bound to a hinge's motor speed reverses the wheel
  each half period (behaviour test); unbinding restores the authored speed.
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
