# Defaults that behave: parameters scaled to the body
Phase 4b · owner direction 2026-11-16 · "try setting values for parameters
and seeing the outcome — set defaults such that editor changes are maximally
intuitive and out of range behaviors only occur when sliders are in extreme
settings. Things like motor torques, spring stiffness, and restitution
coefficients should be appropriately scaled to their body's mass so that this
is ensured. Creatively add additional testing to ensure these more abstract
specifications are maintained." · status: in review

## Goal

For every physical parameter a user edits, do two things:
- the **default** produces a sensible, visible behaviour on the body it
  applies to, at any mass from a pebble to a truck;
- the **slider's range** is such that wild behaviour (explosion, tunnelling,
  jitter, no visible effect) happens only near its ends.

Parameters:
- motor torque/force and target speed;
- spring stiffness, damping and rest length;
- restitution and friction;
- density;
- field strength;
- joint break force;
- anything else the inspector edits.

The method is empirical. Run the sim headless across masses and slider
positions, look at the outcomes, choose the scaling, then **lock it in with
tests** that state the behaviour, not the numbers.

## Read first

- `crates/gradiance-domain/src/joint.rs`: motor `max_torque`/`max_force`
  `<= 0` = **auto** (scaled from mass), the `motor_ceiling_auto_scales…`
  test. Springs: `stiffness` N/m, `damping`. These are the precedents to
  generalise.
- `crates/gradiance-ui/src/inspector.rs`, `joint_inspector.rs`: today's
  ranges and drag speeds. `widgets::precise_drag`.
- `crates/gradiance-physics/` (joint sync, forces, fields).
- `docs/units-decision.md`. Brief 5-1 is retyping these fields into unit
  newtypes in parallel. Build on main after it merges, and use the types.

## Facts (orchestrator, 2026-11-16)

- **Format and UI may change freely** (owner). That opens the cleaner
  design: author springs as **natural frequency (Hz) + damping ratio**, and
  motors as a **response time** or a dimensionless "strength". These are
  mass-invariant, so a spring keeps its feel when the body's density
  changes. Physical N/m and N·m are derived at sync time from the current
  masses. Choose per parameter between:
  - mass-relative authoring;
  - absolute authoring with mass-scaled defaults and slider ranges.
  Justify each in the Outcome. If you change authored fields, bump
  `FORMAT_VERSION`, regenerate goldens, and update verbs and docs.
- Restitution and friction are dimensionless; mass doesn't scale them.
  Their "intuitive" issue is the slider curve: most of the visible change
  happens near restitution 1. Consider a perceptual mapping.
- Derived physics values are rebuilt by `Changed<>` sync systems (invariant
  5). A mass-derived stiffness must be re-derived when mass changes, and is
  never serialized.

## Revision (orchestrator, 2026-11-18)

Measured by the worker against the code at U-7 (4dd0d89), where the Facts
above are stale:

- **There are no sliders.** Every inspector number is an unbounded
  `precise_drag` (`egui::DragValue`) with a fixed speed; nothing clamped
  restitution to `[0, 1]` or friction to `>= 0`. "Slider range" in this
  brief therefore means the commit-time clamp plus a position scale used by
  the tests (`gradiance_domain::tuning::EditRange`).
- Brief 5-1 has merged: the fields are already `gradiance-units` types.
- The spring tool's "mass-based" stiffness used the body's AABB **area**
  (density ignored); the inspector's reset was a fixed 100 N/m; default
  damping 0 (never settles).
- Motor `damping` (default 30, documented 1/s) is passed to avian 0.8 as a
  **per-substep** gain; see the Outcome: the default motor was broken.
- The auto motor ceiling used body A's inertia only (a static ground as
  body A sized the ceiling by the ground).
- Break force reset was a fixed 100 N; field strength defaults to
  -2000 m/s² at the surface scaled by the source's mass.

## Approach

1. **Experiment harness** (test-support code, not shipped UI): spawn a
   canonical rig per parameter, step N seconds headless, and extract
   behaviour metrics:
   - spring oscillation period and decay;
   - motor time-to-target-speed and overshoot;
   - bounce height ratio;
   - slide distance;
   - energy drift;
   - max penetration;
   - NaN/explosion.
   Rigs: spring + mass, motor + wheel, drop onto ground, slope.
2. **Sweep:** body mass over several orders of magnitude (e.g. 0.01 kg to
   1000 kg; size and density both vary), times slider position (0, 10%,
   default, 50%, 90%, 100%). Record a table in the Outcome. Images help:
   plots as PNGs in the scratchpad.
3. Choose the scalings, defaults and slider ranges from the data.
4. **Behaviour tests** (the owner asked for creativity here). Properties
   over the mass sweep:
   - the default is in a "comfortable band" (e.g. spring period 0.3–2 s and
     settles; motor reaches target speed in 0.1–2 s without exceeding 20%
     overshoot; default restitution bounces visibly but decays);
   - the inner 10–90% slider range never explodes or NaNs, and keeps energy
     drift below a bound;
   - monotonic: moving a slider up moves the metric the same direction;
   - mass-invariance (where mass-relative authoring is chosen): the same
     authored value gives the same period at 0.1 kg and 100 kg.
   Use proptest where it fits. These run in CI, so keep the step counts
   modest.
5. Screenshots of a few rigs at default settings in the running app.

## Out of scope

New joint types, solver changes, units display/input (5-3), group workflow
(4b-1/2), visuals (4b-3), `docs/state.md`.

## Done when

- The sweep table and chosen scalings are in the Outcome.
- Behaviour tests green in CI and reasonably fast; note their runtime.
- Inspector defaults and ranges updated, with verbs and docs.
- Screenshots described in the PR. Gates green. Outcome filled.

## Outcome

**In review (2026-11-18, worker).** Stacked on U-7 (#258) and 4b-3 (#257).

### Landed

| Parameter | Before | Now | Behaviour test (`tests/it/defaults.rs`) |
|---|---|---|---|
| Spring | `stiffness` N/m (tool: 100 × AABB area, density ignored; reset 100), `damping` 0 | **mass-relative**: `frequency` Hz (default 1.5, range 0.1–20 log) + `damping_ratio` (default 0.1, range 0–2); N/m derived from the reduced mass every frame, never saved | one period 0.3–2 s on pebble, 1 kg and truck, within 5% of each other, settles (last second < 1/5 of first); density ×100 keeps the period; inner 10–90% stays finite and monotonic |
| Motor gain (`damping`) | 30, passed to avian as a per-substep gain | rate 1/s converted with the substep length, capped at 1 (`tuning::motor_gain`); default 360/s = gain 1 | hinge motor (world pin and on a static ground body) at 90% speed within 0.5 s, no overshoot > 20%, on all masses; arms of 1 m and 10 m hold ≥ 80% speed against gravity; slider motor likewise |
| Motor auto ceiling | `2000 × I(body A)` (static body A: the ground's inertia) | `2000 × reduced inertia` (statics immovable), re-derived when a mass changes (`retune_joints`) | ceiling follows a density change ×1000 |
| Restitution | unbounded drag | commit clamped to [0, 1] | bounce ratio monotonic in e, mass spread < 0.05, never gains height |
| Friction | unbounded drag | commit clamped to [0, 2] | default holds on 25°, slides on 35° (≥ half the Coulomb prediction), 1.8 holds on 40°, all masses |
| Break force | reset 100 N | reset = 10 × weight of the moving connected bodies (`tuning::default_break_force`); drag speed 1% of it | default holds a pendulum released level (pebble, truck); breaks at 20× gravity |

Format **v8** (`FORMAT_VERSION`; v7 rejected, `v7_files_are_rejected`);
goldens regenerated (scene fixture renamed `v8_every_quantity.ron`; replay
goldens change only by `version` and motor `damping: 360.0`). The `spring`
verb is now `(spring a b hz ratio)`; `docs/ops.md`, `docs/scripting.md`,
`docs/units-decision.md`, `docs/agent-context.md` updated. Pure math and
ranges: `crates/gradiance-domain/src/tuning.rs` (unit-tested).
`physics::joint_sync::retune_joints` (PostUpdate, after `sync_joints`)
writes the mass-derived values in place, only on a difference.

Review question **`spring-feel`** (4 options: settling = default, lively,
calm, before) with scenes in `docs/review/scenes/spring-feel/`.

### Decisions

1. Springs mass-relative (frequency + ratio), not N/m with scaled defaults:
   archive entry `spring-authoring`. avian's `JointDamping::linear` is
   already a rate on the reduced mass, so the ratio maps to `2ζω` with no
   mass; only the compliance is mass-derived.
2. Motor rate → per-substep gain, capped at 1: archive entry `motor-gain`.
3. Motors stay **absolute** (rad/s, auto torque) — a hinge's angular speed
   is already scale-free, and the auto ceiling is mass-scaled.
4. Restitution keeps the coefficient (no perceptual remap): the bounce
   height is `≈ 0.77·e²` on every mass, the coefficient is what scripts,
   Algodoo and avian use, and the clamp removes the energy-gaining region.
5. Effective mass of a spring/motor = the reduced mass of the two bodies;
   a static, kinematic or not-yet-computed side is immovable.

### Measured (headless, `defaults::sweep`, 60 Hz, 6 substeps)

Before (U-7 head):

| body | spring period (tool default) | hinge motor at centre, world pin | on static ground (body A) |
|---|---|---|---|
| 0.1 m, 0.01 kg | 0.63 s, never settles | 409 rad/s (target 2) | −273 rad/s |
| 1 m, 1 kg | 0.63 s | 0 (does not turn) | −273 rad/s |
| 5 m ρ4, 100 kg | 1.26 s | 0 | −273 rad/s |
| 10 m ρ10, 1000 kg | 1.98 s | 0 | −273 rad/s |

Restitution bounce ratio (unchanged, mass-invariant): e = 0.3 → 0.05–0.07,
0.5 → 0.19, 0.8 → 0.49, 0.95 → 0.70, 1 → 0.77; never above the drop.
Motor gain sweep (centre wheel, 1 kg): gains 0.1–1.5 track (t90 ≤ 0.07 s),
3 and above stall or spin away.

After:

| mass kg | spring period (1.5 Hz, ζ 0.1) | swing first/last s (fraction of size) | motor t90 world / ground | bounce e=0.3 / e=1 |
|---|---|---|---|---|
| 0.01 | 0.678 | 0.93 / 0.013 | 0.017 / 0.017 s | 0.070 / 0.770 |
| 0.09 | 0.678 | 0.31 / 0.004 | 0.017 / 0.017 | 0.049 / 0.764 |
| 1 | 0.678 | 0.093 / 0.001 | 0.017 / 0.017 | 0.064 / 0.768 |
| 9 | 0.678 | 0.031 / 0.000 | 0.017 / 0.017 | 0.062 / 0.772 |
| 100 | 0.678 | 0.019 / 0.000 | 0.017 / 0.017 | 0.066 / 0.756 |
| 1000 | 0.678 | 0.009 / 0.000 | 0.017 / 0.017 | 0.068 / ~0.77 (within 0.05 of the 1 kg box) |

Spring frequency positions (pebble / truck period): 0.1 and 0.17 Hz (10%)
too slow to complete a period in 3 s but finite; 1.41 Hz (50%) 0.706 /
0.706 s; 11.8 Hz (90%) 0.089 / 0.087 s; 20 Hz 0.05 s. Damping ratio, last
over first swing after 5 s (1 kg): ζ 0 → 0.61 (solver damping only), 0.1 →
0.014, ≥ 0.2 → 0. Motor rate (slowest speed after 0.5 s, 0.3 m / 1 m arm,
target 2): 30/s → −0.86 / 0.26; 120 → 0.48 / 1.53; 360 → 1.48 / 1.84;
1000 → same as 360 (capped).

Runtime: the 10 behaviour tests take 13 s on one container (4 threads);
the sweep 30 s (ignored).

### Not checked

- No screenshots (disk was short; the review scenes stand in).
- Kinematic bodies as motor/spring endpoints (counted immovable).
- `SimSettings::speed` is not in the substep length used by
  `motor_gain`; at speed ≠ 1 the gain is off by that factor (still ≤ 1).

### Cut (and bugs seen, not fixed)

- **Short gravity-loaded arms lose motor speed** (0.3 m holds ~75%, 0.1 m
  ~25%): avian's motor corrects per substep at the centre-of-mass inertia
  while the pivot is off-centre. Needs more substeps or a solver change
  (out of scope). The test asserts ≥ 1 m arms only.
- **World-pin motor sign:** on a world pin the motor drives the pin
  against the body, so a positive rpm turns the body clockwise, opposite
  to a body-to-body hinge. Relates to the retained "world-pin angle sign
  (2-1)"; tests measure the relative speed.
- **Field strength** (`FieldSource`, default −2000 m/s² at the surface
  per reference mass): couples through the source's mass, so a truck-sized
  source pulls ~10⁶ m/s². Not scaled; needs its own measurement.
- Slider motor default speed (2 m/s) is 20 body lengths/s on a pebble and
  0.2 on a truck; a size-scaled default needs the inspector to read the
  body size at toggle time.
- Density and gravity-scale ranges, tracer fade, emissive: unchanged.
- The verbs (`set-restitution`, `set-friction`) do not clamp; only the
  inspector does.

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `-p` tests for domain, physics,
interaction, command, scene, ui, script; `it` modules defaults, joints,
joint_sensors, joint_edit, joint_breaking, physics_sync, persistence,
scripting, replay, op_conformance, review, registry_validation,
reflect_intents, tools, script_examples, unit_commands, rod, client,
remote. One existing test changed its threshold:
`joints::world_pinned_tilted_motor_oscillates` counted direction changes
with a 1e-6 motion threshold; the now dead-beat motor reverses within one
frame (a 1e-5 rad turnaround frame), so the threshold is 1e-3. rustc
1.100.0 = `RUST_PIN`.
