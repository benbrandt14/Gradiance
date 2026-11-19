# Type a value with its unit
Phase 5 · units P4 · "UI SI display + input (display slice landed) …
*Remaining:* unit-aware *input* parsing (`Dimension::parse`) and a
unit-system display toggle." (`docs/units-decision.md` § Phasing 5) ·
status: in review (branch `claude/project-thread-tw1mvl-5-3`, stacked on 5-2)

## Goal

1. A numeric field on a physical quantity accepts a typed value with a unit
   (`25 cm`, `90 deg`, `2 kN`) and stores SI. A bare number means the
   field's display unit.
2. A **workstation** display preference picks how angles show (degrees or
   radians) and, if cheap, a length scale (m / cm / mm). Every inspector and
   settings field honours it.
3. It closes the retained Phase 5 note: the joint inspector shows "angle
   limits (deg)" next to an angle sensor in rad. After this, both follow the
   one preference.

## Read first

- `crates/gradiance-units/src/dimension.rs` ≈L100–140: `Dimension::parse`,
  `ParseError`, symbols.
- `crates/gradiance-ui/src/widgets.rs`: `precise_drag`, `precise_drag_unit`,
  `labelled_drag`. `docs/ui-design.md` on numeric fields.
- `crates/gradiance-ui/src/joint_inspector.rs` (degree limits) and
  `ports.rs` (sensor readout units).
- CLAUDE.md invariant 4: a new settings resource is workstation config and
  is classified in `EnvironmentRecord` (see how brief 4-3 classified
  `CameraSettings`).
- Brief 5-1's Outcome (which fields carry which newtype).

## Facts (orchestrator, 2026-11-16, main `3b8023a`)

- `Dimension::parse` exists. *Unverified:* which suffixes it accepts
  (prefixes like `k`/`c`/`m`? `deg`/`°`?). Extend it in `gradiance-units`,
  as pure, tested code, where needed.
- egui's `DragValue` takes a custom parser/formatter (`custom_parser`,
  `custom_formatter`). *Unverified* in bevy_egui 0.41's egui; check
  `docs/bevy19-notes.md` and the source.
- A unit preference is display only: authored values and the save format
  stay SI. Scripts stay SI; script units are 5-4.

## Approach

1. Parsing in `gradiance-units` (unit tests: every dimension's symbol, SI
   prefixes where meaningful, deg↔rad, errors).
2. A `UnitDisplay` (name yours) workstation resource, classified.
3. `precise_drag_unit` (and whatever field path the reflect grid uses for
   newtypes) parses and formats through it. One place, not per field.
4. Screenshots: joint inspector and a settings field in degrees, then in
   radians; a typed `25 cm` committing 0.25 m.

## Out of scope

Unit *systems* beyond SI scaling (no imperial unless trivially free),
script/DSL units (5-4), frames, styling, `docs/state.md`.

## Done when

- Parse tests; a test that a typed unit value commits the right SI value
  as one undo step (the edit seam); the preference is classified, saved and
  never undone.
- Joint inspector and sensor readouts agree on angle units.
- Screenshots in the PR; `units-decision.md` P4 and roadmap updated; the
  retained "deg vs rad" note can be struck. Gates green. Outcome filled.

## Revision (orchestrator, 2026-11-18)

Written by the 5-3 worker from measurements on `0cce932` (5-2 head),
because the brief predates the operation registry (U-1..U-6) and the theme
tokens (U-7).

- **The parser lives in `gradiance-units`** (`expr.rs`), not
  `gradiance-kernel`. Measured: the kernel has no parser at all (one
  `lib.rs`, the RPN tape), the units crate already owns `Dimension::parse`,
  and both are bottom nodes, so neither can use the other without a new
  DAG edge. The parser needs no bevy; it sits in the units crate because
  the dimension table it fits results to is there. One parser: fields use
  it now; U-8 (Ctrl snap, drag wrap, Backspace = default) and 5-4 (script
  units) are to call `gradiance_units::expr::evaluate` /
  `DisplayUnit::parse`, not write a second one. "Console and kernel" in
  the plan (§ 7.2) means: the console gets it through 5-4's verbs; the
  kernel never parses (Tier B).
- **Which widgets take it.** 81 numeric-field sites in `gradiance-ui`.
  The quantity ones go through three entry points: `precise_drag_unit`
  (edit seam: inspector, joint inspector, depth), `labelled_quantity`
  (config seam: settings) and the reflect grid's leaf (settings tabs). All
  three call one function, `ui::unit_fields::drag_value`. Unit strings at
  call sites become a `Dimension` (or a stored `DisplayUnit` for a
  settings value kept in degrees). Plain numbers (`precise_drag`, the
  reflect grid's `f32`) parse maths through the same parser.
- **The preference** is a resource in `domain::settings` (`UnitSettings`),
  initialised by `gradiance-interaction` beside `ChromeSettings`, with
  operations `units.display` / `units.shown` (one surface, plan § 0). The
  UI publishes it into the egui context once per frame, so no panel
  threads it.
- **Out of scope, as U-8's:** drag wrap, Ctrl snapping, Backspace = default,
  `-` negates, right-click Copy/Paste/Reset, an error tooltip on a bad
  entry.

## Outcome

Branch `claude/project-thread-tw1mvl-5-3`, stacked on 5-2 (#262) → #261 →
#260 → #259 → #258 → #257.

### Landed

- **One typed-value parser**, `gradiance_units::expr` (pure, no bevy
  use): numbers (`1.5e3`, `.5`), `+ - * / ^`, brackets, unary minus,
  superscripts `²` `³`, the typographic `· × ÷ −`, juxtaposition
  (`25 cm`, `2 pi`, `3 (1 + 1)`), constants `pi` `tau`, and a unit table
  (m km cm mm um in ft; kg g mg t; s ms min h; rad mrad deg ° rev turn; N
  mN kN MN; J mJ kJ; Hz kHz; rpm). A value carries exponents of m, kg, s
  and rad (`Dims`); adding unlike dimensions is an error.
  `Dimension::{ALL, dims, of_quantity}`; `Dimension::parse` now uses it
  (the old suffix-strip `ParseError` is gone, its only user was a comment).
- **Display units**, `gradiance_units::display`: `UnitPrefs { angle:
  Degrees | Radians, length: Metres | Centimetres | Millimetres }` →
  `DisplayUnit { dimension, symbol, per_si }` with `format`, `parse` (a
  bare number is the shown unit; a unit must match the dimension) and
  constants `DEGREES`, `DEGREES_PER_S`, `RPM`. Angles cover angular
  velocity (°/s or rad/s); the length scale covers lengths only.
- **The preference**: `domain::settings::UnitSettings`, operations
  `units.display` `(display-units angle length)` (config) and
  `units.shown` `(units-shown)` (query); Settings > Interface > Units.
- **Fields**: `ui::unit_fields` (`publish_unit_prefs` runs after
  `apply_chrome`; `drag_value`, `stored_drag`). `precise_drag_unit` takes a
  `Dimension`; `precise_drag_in` takes a fixed unit (the hinge motor's
  rpm); `labelled_quantity` is the config-seam row. The reflect grid finds
  a quantity newtype's dimension by type name, and a plain `f32` kept in
  degrees by a table row (`rotation_step_deg`, `perspective_deg`). Callers
  converted: inspector (density, shape, depth, rod readout), joint
  inspector (limits, motor, break force, spring), settings (gravity
  direction/strength, timestep, shadow reach, grid, snap, scenery).
- **Readouts follow the same preference**: the sensor rows and the mass
  row (`ports::readout`) and the live plot (series scaled and labelled in
  the shown unit; history stays SI). The joint inspector's limits and an
  angle sensor now read in one unit (closes the retained "deg vs rad"
  note; it was not in `state.md` any more).
- **Review question `units-display`** (which default: degrees + metres,
  radians + metres, degrees + centimetres), scenes in
  `docs/review/scenes/units-display/`, `expected_bodies` row.
- Docs: `units-decision.md` P4 and the architecture sketch, `roadmap.md`
  P4 ticked, `ui-design.md` (quantity fields name a dimension),
  `scripting.md` (the two verbs), `docs/ops.md` regenerated.

### Decisions

- **Not saved with the scene** (archive `units-display-saved`,
  `decision: true`): workstation config like the chrome preset, so a
  scene from someone else does not switch your fields. The brief said
  "saved"; it predates U-7's precedent. It moves to the workstation file
  with the chrome preset (U-10). Never undone (test).
- **Default degrees + metres**: the joint limits were already degrees
  and lengths were metres, so nothing a user saw changes by default. The
  owner can pick otherwise in review (`units-display`).
- **Angle is its own base dimension** in the parser, so `2 Hz` is refused
  in an angular-velocity field and `25 cm` in an angle. Torque and energy
  share exponents (N·m = J), so `5 J` types into a torque field.
- **A bare number means the shown unit**, also for constants: `pi` in a
  degree field is 3.14°; `pi rad` is 180°.
- **A bad entry keeps the old value** (egui's behaviour when the parser
  returns nothing) and commits nothing (test). Showing why is U-8's.
- **The hinge motor stays in rpm** whatever the angle preference (A-2's
  "20 rpm"), and accepts `2 rad/s` or `90 deg/s`.
- **Camera orbit speed (°/px) is left as degrees per screen point**: a
  rate per pixel is not a `Dimension`. The light gadget has no number
  field.
- **Imperial**: `in` and `ft` only (two table rows, free); nothing else.
- **No external parse crate** (ruling 1): ~350 lines hand-written; the
  only `Cargo.lock` change is `proptest` as a dev-dependency of
  `gradiance-units` (already in the workspace).

### Measured

- Parser: unit tests for every unit, every `Dimension` symbol reading
  back as its dimension, errors; proptests (arbitrary text never panics;
  grammar soup never panics; number × unit round-trips; arithmetic equals
  f64). `of_quantity` checked against every quantity type's `UNIT`.
- Edit seam: `unit_fields::tests::a_typed_unit_value_commits_its_si_value_once`
  (headless egui: click, select all, type `25 cm`, Enter → exactly one
  `Commit::Done(1.0, 0.25)`; one commit is one intent, one undo step), a
  bare number under a cm preference stores metres, a wrong entry commits
  nothing.
- Workstation config: `tests/it/units_display.rs` (no undo step, undo
  leaves it, not in `EnvironmentRecord`, unknown names list the choices).
- Screenshots (not in git; `/mnt/project-files/5-3/`, README there):
  settings and properties in degrees + metres, then radians + centimetres;
  the joint window with limits and the angle sensor in radians, then
  degrees.

### Not checked

- Typing `25 cm` live in the running app: the screenshot harness's
  injected Backspace/Delete/Ctrl+A did not reach a focused egui text
  field (text was appended to the old value, and correctly refused). The
  headless egui test covers the commit; the harness gap is a U-4 follow-up.
- The full `cargo test --workspace` (CI).

### Cut

- Script/console units (`(cm 25)`, a `units.parse` verb): brief 5-4,
  which should call `DisplayUnit::parse`.
- U-8's widget behaviours, including an error hint on a bad entry.
- A length scale for velocity/area/density (cm/s, cm²): the preference
  scales lengths only; a per-dimension table would be the next step.
- `orbit_speed_deg` and other per-pixel rates stay in degrees.

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p gradiance-units`,
`-p gradiance-ui --lib`, `-p gradiance-scene --lib review`; `cargo test
--test it` filtered to `units_display::`, `op_conformance::`, `review::`,
`ui_snapshots::`, `ui_panels::`, `ui_registry::`, `joint_sensors::`,
`signals::`, `quantities::`, `look::` and `settings` (with
`GRADIANCE_REQUIRE_GPU=1`). All green. The `settings_grid` UI snapshot was
regenerated (fields now show their units). rustc 1.100.0 (the pin).
