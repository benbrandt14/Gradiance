# Authored quantities carry their units
Phase 5 · units P2·types · "retype geometry/physics/domain (by accretion).
Thread the `gradiance-units` newtypes through the pure layers and the physics
boundary … Pure refactor over the already-SI values from the flip — no value
or format change." (`docs/units-decision.md` § Phasing 3) · status: returned

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

**Returned before any retype (2026-11-16, worker).** The first unverified
fact is false: a `gradiance-units` newtype does **not** serialize
transparently, so retyping any persisted field changes the scene format.
Per the brief ("stop and report rather than bumping the version") no code
was changed. A ruling is needed before this brief can proceed.

### Evidence

- Probe against the pinned `ron = 0.12.2` with `PrettyConfig::new()` (what
  `scene::to_ron` uses), a struct with a `Length(pub f32)` field derived
  exactly as `scalar_quantity!` derives it (`Serialize, Deserialize`, no
  `#[serde(transparent)]`):
  - `f32` field saves as `near: 0.5`; `Length` field saves as `near: (0.5)`.
  - Loading `(near: 0.5)` into the `Length` struct fails:
    `ExpectedNamedStructLike("Length")`. So an existing v6 file would stop
    loading after a retype, not just re-save differently.
  - With `#[serde(transparent)]` the field saves as `0.5`, but then loading
    `(0.5)` fails (`ExpectedFloat`).
- The repo already shows the parenthesised form: the replay goldens carry
  `density: (1.0)`, `gravity_scale: (1.0)` (avian newtypes) and the
  already-typed joint motor fields `target_velocity: (2.0)`,
  `max_torque: (0.0)` (`tests/it/golden/replay_joints.ron:111-112`).
  `JointCommon::break_force: Option<Force>` saves as `Some((x))` likewise.
  So the format already has two spellings for a scalar: bare for domain
  `f32`s, parenthesised for every newtype.

The second unverified fact, for completeness: `reflect_grid::leaf_widget`
(`crates/gradiance-ui/src/reflect_grid.rs:78`) handles `f32`, `bool`, `u32`,
`Vec2` and nested `Struct`; a single-field tuple struct falls through to
`(unsupported)`. So a retyped settings field would render as a non-editable
label until the brief's in-scope unwrap fix lands.

### Options for the ruling

1. **Keep the format: per-field serde adapter.** Add
   `gradiance_units::serde_scalar` (and a `Vec2` twin) and mark each retyped
   persisted field `#[serde(with = ...)]`, so it stays a bare number on
   disk. No version change, goldens byte-identical; the existing joint
   fields keep `(x)`. Cost: one attribute per field, and forgetting it
   silently breaks loading (the brief's round-trip golden must set every
   retyped field to catch that). This is the option that fits the brief as
   written.
2. **Accept the parenthesised form for retyped fields** (consistent with
   avian's components and the joint motors). A format change: needs a
   version bump (v7, v6 rejected, per decision 2's persistence-debt
   stance) or a lenient deserializer accepting both spellings. Goldens
   change.
3. **`#[serde(transparent)]` in both quantity macros.** Every typed quantity
   becomes a bare number, the cleanest end state, but it changes the
   *already-typed* joint fields (`(2.0)` to `2.0`), so it is also a format
   change (bump or lenient load), and `replay_joints.ron` changes.

Non-persisted retypes (live-only values, e.g. `JointSensors`) are
unaffected by any of this, but almost every field the brief lists is in a
saved record (`BodyRecord` embeds `DepthBand`/`Appearance`/`FieldSource`/
`Tracer`; `EnvironmentRecord` embeds `SimSettings` and the signal
resources directly), so splitting them out would not make the brief land.

No classification table was produced: it depends on the ruling (under
option 1 every retyped persisted field also needs its adapter noted).
