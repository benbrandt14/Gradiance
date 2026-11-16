# Units in scripts
Phase 5 · units P6 · "units in the parameter-linking / expression DSL …
Dimensional quantities in parameter-linking menus; adopt an external parse
crate behind the units API if multi-unit entry is wanted."
(`docs/units-decision.md` § Phasing 6) · status: ready (after 5-3)

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

*(worker fills in)*
