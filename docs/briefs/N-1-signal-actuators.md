# Signals that drive physics
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

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

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-n1`.

**Landed:**

- `gradiance-domain/src/signal.rs`: `SignalSink` gains `MotorSpeed`,
  `MotorCap` (keyed by the joint), `FieldStrength`, `GravityScale` (keyed by
  the body); `actuator()` names the target (`ActuatorTarget`),
  `dimension(linear)` gives the unit (rad/s or m/s, N·m or N, m/s², ratio),
  `label()` the channel name. Remapped on duplication like the colour sinks.
  Additive variants: old scenes load unchanged, no format bump.
- `gradiance-physics/src/drive.rs` (new): the derived `SignalDrive`
  component (only the driven fields are `Some`), `undriven_target`,
  `gravity_correction` and `drive_gravity_scale`. Physics reads the drive in
  place of the authored value: `retune_joints` writes the driven motor speed
  and cap into the live engine joint and restores the authored ones when
  undriven; `Fields::contributions` uses a driven strength;
  `drive_gravity_scale` adds avian's `ConstantLinearAcceleration` for
  `gravity × (driven − authored)` and removes it after, so the authored
  `GravityScale` is never written. `drive_oscillating_motors` leaves a
  speed-driven motor alone. Defined in physics because physics reads it and
  signal already depends on physics: **no new dependency edge**.
- `gradiance-signal`: `drive_actuators`, chained after `evaluate_signals`,
  is the one actuator system: it recomputes each live drive
  (`desired_drive`), writes it only on a difference, removes a drive no
  binding sets, inserts one for a newly bound target. A non-finite value
  never reaches the solver. The computed-signal path that feeds it no
  longer allocates per frame (`evaluate_computed` uses a `[f32; MAX_VARS]`
  stack array; `publish_sensor_refs` and the bus hygiene read the compiled
  input lists instead of `expr.inputs()`).
- UI: `ports::body_actuators` adds `field` and `gravity`;
  `ports::joint_actuators` gives `motor speed` and `motor cap`. The node
  graph gets a minimal **joint block** (`GraphKey::Joint`): joint sensors as
  outputs, motor pins as inputs with units, shown when a wire names the
  joint or when it is a motorised hinge or slider on a body already on the
  canvas. The Signals list hides domain, gradient and curve for an actuator
  binding ("raw value") and offers the matching sinks.
- Tests: `tests/it/actuators.rs` —
  `a_sine_signal_on_a_hinge_motor_reverses_the_wheel_each_half_period`
  (the "done when": `(defsignal "osc" "t 3.14159265 * sin 4 *")` through
  `signal.computed`, bound to a world-pinned wheel's motor speed; at each
  half period's midpoint the wheel turns the signal's way at its speed and
  the engine target equals the signal; authored `JointDef` unchanged and no
  undo step over 300 frames; unbinding removes the drive and the wheel
  settles at the authored 1 rad/s), `a_motor_cap_drive_limits_the_motor`,
  `a_gravity_scale_drive_floats_a_body_until_unbound`,
  `a_field_strength_drive_replaces_the_authored_strength`. Unit tests in
  domain (sink targets, units, remap), physics (undriven target, gravity
  correction), signal (`desired_drive`) and ui (pin order, a motor wire into
  a joint block).
- Docs: `docs/signal-dataflow.md` (actuator sinks, `SignalDrive` row).
  `docs/ops.md` is unchanged (no operation added).

**Per-frame cost:** `drive_actuators` is `O(D·B + A·B)` id comparisons
(`D` live drives, `A` actuator bindings, `B` bindings) plus one bus-name
scan per match; no allocation in the steady state (commands only when a
drive appears or goes). Physics adds one `Option<&SignalDrive>` read per
joint in `retune_joints` and per field source in `Fields`, and
`drive_gravity_scale` visits only driven or corrected bodies.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`actuator-raw-value`, `gravity-scale-drive`, `motor-cap-drive` (a driven cap
is used as is; `<= 0` switches the motor off because avian reads a
non-positive cap as unlimited), `actuator-precedence` (drive beats
oscillation; never enables or creates a motor or field),
`joint-blocks-in-graph` (a minimal joint block — this touches the retained
"joint node-graph block (2-1)" item only as far as decision 2 needs),
`actuator-script-verb` (no binding verb in N-1).

**Not done / follow-ups:** no `signal.bind` / `signal.unbind` operation
(bindings are made in the node graph and the Signals list, and saved with
the scene; N-2/N-3 tests may want the verb). The joint block has no
"add to node editor" entry of its own. The canvas pins were not checked on
screen (no screenshot run).
