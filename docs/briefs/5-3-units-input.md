# Type a value with its unit
Phase 5 · units P4 · "UI SI display + input (display slice landed) …
*Remaining:* unit-aware *input* parsing (`Dimension::parse`) and a
unit-system display toggle." (`docs/units-decision.md` § Phasing 5) ·
status: ready (after 5-1)

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

## Outcome

*(worker fills in)*
