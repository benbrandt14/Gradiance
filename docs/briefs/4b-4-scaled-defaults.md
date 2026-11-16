# Defaults that behave: parameters scaled to the body
Phase 4b · owner direction 2026-11-16 · "try setting values for parameters
and seeing the outcome — set defaults such that editor changes are maximally
intuitive and out of range behaviors only occur when sliders are in extreme
settings. Things like motor torques, spring stiffness, and restitution
coefficients should be appropriately scaled to their body's mass so that this
is ensured. Creatively add additional testing to ensure these more abstract
specifications are maintained." · status: ready

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

*(worker fills in)*
