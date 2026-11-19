# Units in scripts
Phase 5 · units P6 · "units in the parameter-linking / expression DSL …
Dimensional quantities in parameter-linking menus; adopt an external parse
crate behind the units API if multi-unit entry is wanted."
(`docs/units-decision.md` § Phasing 6) · status: in review (branch
`claude/project-thread-tw1mvl`, PR #266)

## Goal

A script can say what unit a number is in, and a scripted value carries its
dimension far enough to catch a mistake. The minimum:
- a quantity literal or constructor that converts to SI, e.g.
  `(cm 25)` → 0.25, or `(q 25 "cm")`;
- query verbs that can report a value's dimension, e.g.
  `(quantity-unit "speed")` → `"m/s"` with brief 5-2's catalog.

The bar is "a user can write a scene in the units they think in, and the
docs show it", not a dimensional type system in steel.

## Read first

- `docs/units-decision.md` ruling 1 (≈L91: "adopted when the in-scope
  DSL-units work (P6) needs multi-unit entry") and § Phasing 6.
- `docs/script-lisp-decision.md` (the two tiers). The numeric kernel
  (`gradiance-kernel`) must stay pure and allocation-free. Decide whether
  units touch the kernel at all (they probably shouldn't: convert at the
  authoring boundary).
- `crates/gradiance-units/src/dimension.rs` (5-3 extended parsing there).
- Briefs 5-2 and 5-3 Outcomes.

## Facts (orchestrator, 2026-11-16, main `3b8023a`)

- Spawn verbs take SI (metres, radians) since the P2 flip and 3-6.
  Changing that is out: existing scripts must keep working unchanged.
- "Parameter-linking menus" means the signal/node-graph parameter fields.
  *Unverified:* whether those already accept expressions, and through
  which parser. Report what exists; extend only if a unit literal fits
  without a new language.
- An external parse crate is allowed **only** behind the `gradiance-units`
  API and **only** if multi-unit entry needs it. A lockfile change is a
  reviewed change: justify it in the PR, keep the diff minimal.

## Approach

1. Conversion verbs or literals that reuse 5-3's parser (one parser, not
   two). `OpSpec` rows.
2. Dimension query verbs over 5-2's catalog.
3. If the node-graph parameter fields take expressions, the same unit
   literals work there. Otherwise record it as a follow-up.
4. `docs/scripting.md` § Units: a short example scene written in mixed units.
   The 3-6 guard test (`tests/it/script_examples.rs`) must keep passing,
   since it runs every doc example.

## Out of scope

A dimensional type checker in steel, units inside `gradiance-kernel`
tapes, changing verb argument units, imperial unless free, `docs/state.md`.

## Done when

- Tests: conversions round-trip through 5-3's parser; a bad unit is a
  visible script error; a mixed-unit doc example spawns the same scene as
  its SI twin.
- Docs and `units-decision.md` P6 / roadmap updated. Gates green. Outcome
  filled.

## Outcome

Branch `claude/project-thread-tw1mvl` (PR #266), on main 3f211ab.

### Landed

- **`gradiance_units::convert`** (pure, tested): `to_si(value, unit)`,
  `convert(value, from, to)` / `convert_values` (refuses two units of
  different dimensions, `ExprError::WrongDimension`), `si_unit(Dimension)`,
  `si_symbol(unit)` (the SI symbol of what a unit measures; a canonical
  symbol answers itself so `J` reads `J`, not `N·m`), `known_units()`.
  Every function calls 5-3's `expr::evaluate`: one parser for fields and
  scripts.
- **Three operations** (`crates/gradiance-command/src/ops/units.rs`,
  `Seam::Query`, pure, no snapshot read): `units.si` `(q 25 "cm")` → 0.25;
  `units.convert` `(convert 0.25 "m" "cm")` → 25, where a catalog quantity
  name stands for its SI unit (`(convert 90 "km/h" "speed")` → 25,
  `(convert 90 "deg" "height")` → error); `units.of` `(unit-of "km/h")` →
  `"m/s"`, in `quantity-unit`'s symbols. A parser error is
  `OpError::Invalid` quoting the unit; an unknown unit lists the table.
  `docs/ops.md` regenerated.
- **Dimension query:** `(quantity-unit name)` already landed in 5-2; `unit-of`
  is its counterpart for a unit, and the two compare equal for every
  catalog row (test).
- **Docs:** `docs/scripting.md` verb reference lines and a `## Units`
  section with a mixed-unit scene and its SI twin (both run by
  `script_examples`); `units-decision.md` P6 and the architecture sketch;
  `roadmap.md` P6 ticked with its "not done".

### Decisions (archive, `decision: true`)

- `script-unit-verbs`: one verb with the unit as text (`q`), not one verb
  per unit. The unit table holds `min`, `t`, `g`, `in`, `h`, `s`, which
  scripts already use as names (`min` is a Scheme built-in), and a
  composed unit (`km/h`) would have no verb. Short names are one `define`
  each; the docs show `(define (cm x) (q x "cm"))`.
- `script-unit-check`: the dimension is checked at a conversion, not
  carried by a steel value. A converted number is plain again, so
  `(spawn-circle 0 0 (q 90 "deg"))` is not caught (documented). A tagged
  value type would be the dimensional type system the brief rules out.
- **Units do not touch the kernel.** They convert at the authoring
  boundary; `gradiance-kernel` and `SignalExpr` are unchanged.
- **No external crate, no lockfile change.** Ruling 1's trigger
  ("multi-unit entry needs it") did not fire: the 5-3 parser reads every
  unit the verbs need.

### Measured

- Node-graph "parameter-linking" fields: the block constants (gain `k`,
  oscillator `amp`/`freq`, constant `value`) and the bindings' `[in_min,
  in_max]` are plain `egui::DragValue`s (`crates/gradiance-ui/src/node_graph.rs`
  ≈L1339–1397, `signals.rs` ≈L293) with no parser, and `SignalExpr` is
  built from blocks, not typed text. There is no expression field for a
  unit literal to go into, so step 3 is a follow-up (below).
- `gradiance-units`: every unit in the table converts to SI equal to the
  field parser reading `"{value} {unit}"` and converts back (round trip);
  mismatch and bad-unit errors; `si_symbol` for every `Dimension`.
- `tests/it/script_units.rs`: the operations agree with `evaluate` and
  round-trip through `unit-of`; a script spawns from `q`/`convert`
  values; `q` with `furlong`, `convert` deg→height and m→s, and a malformed
  unit each fail the run with the reason in the console log and spawn
  nothing; the docs' mixed-unit scene spawns the same four bodies (bounds,
  position, rotation to 1e-5) as its SI twin.

### Not checked

- The full `cargo test --workspace` (CI runs it).
- The verbs in the running app's console (headless script path only).

### For later

- Number fields for the node graph's block constants and binding ranges
  through `ui::unit_fields::drag_value`, with a dimension per port
  (plain-number maths at least; unit literals where the port has a
  dimension).
- A unit-carrying value in steel, or spawn verbs that accept a unit, if a
  stronger check is ever wanted (out of scope here).

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p gradiance-units`,
`-p gradiance-script`, `-p gradiance-scene --lib review` (the archive
parses); `cargo test --test it` filtered to `script_units::`,
`script_examples::`, `scripting::`, `op_conformance::`,
`registry_validation::`, `quantities::`, `remote::`, `client::`. All
green. rustc 1.100.0 (the pin). `Cargo.lock` unchanged.
