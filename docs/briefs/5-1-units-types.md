# Authored quantities carry their units
Phase 5 · units P2·types · "retype geometry/physics/domain (by accretion).
Thread the `gradiance-units` newtypes through the pure layers and the physics
boundary … Pure refactor over the already-SI values from the flip — no value
or format change." (`docs/units-decision.md` § Phasing 3) · status: done

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

**Done (2026-11-16, worker).** Every authored physical scalar in
`gradiance-domain` and the settings resources is a `gradiance-units`
quantity. Values are unchanged; **the scene format is now v7** (see the
rulings). Scripts and the settings UI see the same numbers as before.

### Rulings (how the brief changed)

1. *First pass, returned:* the brief's first unverified fact was false. A
   quantity newtype derived `Serialize` saves wrapped (`near: (0.5)`), where
   the `f32` it replaces saves bare (`near: 0.5`), and neither spelling loads
   into the other (probe against the pinned `ron 0.12.2`). Already in the
   repo: the joint motor fields saved `target_velocity: (2.0)`, so v6 had
   two spellings for a scalar.
2. *Ruling 1 (superseded):* keep v6 bytes with per-field `serde_bare`
   adapters. Built and tested, then dropped (it stays in history: commits
   `c7f4fe2`..`0d4fda9`).
3. *Ruling 2 (final, owner: the save format and UI are experimental):* both
   quantity macros are `#[serde(transparent)]`, so **every** quantity saves
   bare, the joint motor fields and `break_force` included. `FORMAT_VERSION`
   6 to **7**; v6 files are rejected through the existing version check
   (`v6_files_are_rejected`, beside the v5 test); no migration. The goldens
   were **regenerated on purpose**: the four replay goldens and the scene
   fixture change only by `version: 7` and `target_velocity`/`max_torque`/
   `max_force`/`break_force` losing their parentheses. avian's own
   `ColliderDensity`/`GravityScale` are outside our macros and keep avian's
   `density: (1.0)`; wrapping them would mean a serde shim on avian types,
   so they were left. Recorded in `docs/units-decision.md` ruling 2.

### Classification

Retyped (all bare on disk):

| Field | Was | Now |
|---|---|---|
| `DepthBand::near`, `far` | `f32` m | `Length` (+ `DepthBand::metres(near, far)`) |
| `FieldSource::strength` | `f32` m/s² | `Acceleration` |
| `Tracer::fade_secs` | `f32` s | `Time` |
| `Tracer::size` | `f32` m | `Length` |
| `NodeAttachment::offset` | `Vec2` m | `Displacement` |
| `JointDef::anchor_a`, `anchor_b` | `Vec2` m | `Displacement` |
| `JointDef::rest_rot_a`, `rest_rot_b` | `f32` rad | `Angle` |
| `JointKind::Hinge::limits` | `Option<[f32; 2]>` rad | `Option<[Angle; 2]>` |
| `JointKind::Slider::limits` | `Option<[f32; 2]>` m | `Option<[Length; 2]>` |
| `JointKind::Spring::rest_length` | `f32` m | `Length` |
| `JointKind::Spring::stiffness` | `f32` N/m | `Stiffness` |
| `JointKind::Spring::damping` | `f32` | `Damping` (the inspector already labels it N·s/m) |
| `JointKind::Spring::range` | `Option<[f32; 2]>` m | `Option<[Length; 2]>` |
| `SimSettings::gravity` | `Vec2` m/s² | `Acceleration2` |
| `SimSettings::timestep_hz` | `f32` Hz | `Frequency` |
| `GridSettings::spacing` | `f32` m | `Length` |
| `GridSettings::origin` | `Vec2` m | `Displacement` |
| `GridSettings::rotation` | `f32` rad | `Angle` |
| `LightingSettings::shadow_distance` | `f32` m | `Length` |
| `ScenerySettings::back_offset` | `f32` m | `Length` |

Already typed before this brief: the motor `target_velocity`/`max_torque`/
`max_force` and `JointCommon::break_force` (now bare too).

Left `f32`/`Vec2`, and why:

| Field | Why |
|---|---|
| `Rgba::r/g/b/a`, `Appearance::emissive` | colour channels / a glow factor |
| `AngularMotorDef::damping`, `LinearMotorDef::damping` | a velocity-tracking gain (1/s), a controller tuning knob; no matching quantity, and `Frequency` would misname it |
| `JointKind::Slider::axis` | a unit direction (dimensionless) |
| `SnapConfig::max_screen_distance` | screen pixels, not world |
| `SnapConfig::rotation_step_deg`, `CameraSettings::orbit_speed_deg`, `KeyLightSettings::azimuth_deg`/`elevation_deg`, `ScenerySettings::perspective_deg` | degree-valued UI angles; `Angle` is radians, so retyping changes stored values (the deg/rad preference is brief 5-3) |
| `CameraSettings::zoom_step`, `pan_speed` | UI feel factors (ratio; screen px/s) |
| `SimSettings::speed`, `plane_friction` | dimensionless |
| `RenderSettings::rim_strength`/`white_point`/`specular`, `LightingSettings::ambient` | render factors |
| `KeyLightSettings::illuminance` | lux: no quantity type, and it is a render brightness |
| `SignalMap::in_min/in_max`, `SignalParam::value/min/max`, `BlockOp` `value`/`amp`/`freq`/`k` | signal-graph values take the dimension of whatever is wired in; typed once the DSL carries units (P6, 5-4) |

Not touched (out of scope by the brief): `geometry` signatures (`ShapeDef`
dimensions, `PosRot`, snapping). Consumers call `.value()` at that boundary.
Derived helpers on `DepthBand` (`z_front`, `thickness`, `bits`) stay `f32`
render/collision numbers. `SimSettings::gravity_polar` stays `(deg, m/s²)`
for the dial widget; `gravity_from_polar` returns `Acceleration2`.

### Seams

- `reflect_grid` (in scope): a single-field tuple struct rendered
  `(unsupported)`; it now recurses into its inner leaf. Headless egui test.
- `reflect_bridge`: `"gravity.y"` stopped resolving once `gravity` was a
  newtype (bevy paths do not step through tuple structs). Path segments now
  retry on the wrapped value; `"gravity.0.x"` still works.
- New DAG edges `command`, `interaction`, `script` to `units` (rows in
  `tests/boundaries.rs`; units is a bottom node any layer may use per the
  decision record). `scene` builds typed values in its test by parsing, so no
  `scene` edge. `ron` is a new dev-dependency of `gradiance-units` (one
  `Cargo.lock` line each for these).

### Evidence

- Format: `crates/gradiance-scene/src/format_golden.rs` sets every retyped
  field to a distinctive value, compares the save byte-for-byte with
  `golden/v7_every_quantity.ron`, and reloads it. The structural guard walks
  a `SceneRecord` by reflection, plants a sentinel in every
  `gradiance_units` value it reaches, and fails if any saves wrapped (proved
  by temporarily dropping an adapter / `transparent`: it named the field).
  `quantity.rs` tests transparency directly.
- Scripts: `reflect_bridge::retyped_domain_quantities_stay_numbers` (the real
  retyped types) and `it::scripting::retyped_quantities_read_and_write_as_numbers`
  (`sim-get`/`sim-set` on `gravity.*`/`timestep_hz`, the `spring` verb).
  There are no joint/depth property verbs yet (retained list), so the spring
  verb is the property path covered.
- UI: screenshots (not in git) of Settings > Simulation (gravity 9.5 m/s²,
  `x 0.00 y -9.50`, timestep 120 Hz, as the start-up script set them),
  Settings > Grid & Snap (spacing 1.0 m, origin 0.0 0.0, rotation 0.0, now
  editable numbers through `reflect_grid`), and the Properties inspector
  (depth front 0.000 m, back 0.100 m).

### Follow-ups (not done here)

- Pixel-era values the flip missed (value changes, so out of scope):
  tracer dots are floored at `size.max(0.5)` m in `render::tracer`, so the
  0.02 m default draws as 0.5 m dots; the node editor's tracer size range is
  `0.5..=20.0` m (`ui::widgets::node_kind_editor`); the slider limit drag
  keeps a 1.0 m minimum gap (`interaction::joint_edit::dragged_limits`).
- avian's `JointDamping::linear` is a relative-velocity damping coefficient;
  check whether its unit really is N·s/m (the inspector label and `Damping`
  type assume so).
- A crate-wide `geometry` retype (shape dimensions, `PosRot`) if wanted.
- `reflect_to_steel` still returns `Void` for enums (so `JointDef.kind` dumps
  as `Void`), as before this brief.
