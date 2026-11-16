# Authored quantities carry their units
Phase 5 · units P2·types · "retype geometry/physics/domain (by accretion).
Thread the `gradiance-units` newtypes through the pure layers and the physics
boundary … Pure refactor over the already-SI values from the flip — no value
or format change." (`docs/units-decision.md` § Phasing 3) · status: ready

## Goal

Every **authored physical scalar** in `gradiance-domain` and the settings
resources is a `gradiance-units` newtype (`Length`, `Angle`, `Density`,
`Acceleration2`, …), not a bare `f32`/`Vec2`. Values, the save format and
script behaviour are all unchanged. The type now says what the number is, so
a later display or parse layer (P4, P6) reads the unit off the field instead
of a hand-kept table.

## Read first

- `docs/units-decision.md` (all of it; rulings 1–2 and § Phasing 3).
- `crates/gradiance-units/src/quantity.rs`: the `scalar_quantity!` /
  `vector_quantity!` macros (serde and reflect shape of a newtype).
- `crates/gradiance-domain/src/joint.rs`: the precedent. Joints already use
  `Angle`, `Force`, `Length`, `AngularVelocity`, `Torque`, `Velocity`.
- `crates/gradiance-script/src/reflect_bridge.rs`: P0 unwraps single-field
  newtypes to scalars, so a retyped field must still read/write from a
  script as a number.
- `crates/gradiance-ui/src/reflect_grid.rs`: the settings grids render
  fields by reflection. Check what a newtype field renders as.

## Facts (orchestrator, 2026-11-16, main `3b8023a`)

- About 60 `pub …: f32` / `Vec2` fields remain in `gradiance-domain`, across
  `depth.rs`, `field.rs`, `tracer.rs`, `settings.rs`, `signal.rs`,
  `appearance.rs` and `joint.rs` (remaining ones). Many are not physical
  (colours, UI factors, `Dimensionless` ratios). **Classify each** in the
  Outcome: retyped (to what), or left `f32` (why).
- *Unverified:* whether the newtypes serialize transparently, so a v6 RON
  file round-trips byte-identical after a retype. A golden or round-trip test
  must prove it, field by field if needed. **The scene format must not
  change**; if a newtype doesn't serialize transparently, stop and report
  rather than bumping the version.
- *Unverified:* whether `reflect_grid` renders a newtype field as an editable
  number. If it doesn't, the minimal fix in `reflect_grid` (unwrap the
  single-field tuple, as the script bridge does) is in scope.
- `geometry` and `physics` signatures are in scope only where an authored
  field's type forces it. A crate-wide geometry retype is not this brief.

## Approach

1. Inventory and classify (an Outcome table).
2. Retype in small commits per file. Each commit stays green.
3. Prove the format: a test that loads a v6 golden scene with every retyped
   field set, saves it, and compares. Prove scripts: `sim-get`/`sim-set`,
   and the property verbs on retyped fields, still take and return numbers.
4. Prove the UI: a screenshot of Settings → Simulation and the inspector
   after the retype, showing values unchanged.

## Out of scope

Display formatting or parsing changes (P4, brief 5-3), the named catalog
(P3, 5-2), DSL units (P6, 5-4), any value or format change, frames,
`docs/state.md`.

## Done when

- Classification table in the Outcome.
- Format-stability and script-compatibility tests; gates green.
- Screenshot described in the PR (not in git).
- `docs/units-decision.md` § Phasing 3 and roadmap P2·types updated (landed,
  or what is left and why).

## Outcome

*(worker fills in)*
