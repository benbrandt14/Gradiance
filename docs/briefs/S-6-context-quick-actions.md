# Context menu quick actions
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("loosen", "centre axle", "centre thruster", "transform into circle", "brake") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

One-click actions on a body: drop all its joints, pin it to the world at
its centre, put a thruster at its centre, turn it into a circle of equal
area, and brake a hinge motor. Each is an operation, each is one undo step.

## Read first

- `crates/gradiance-ui/src/context_menu.rs` — the body section.
- `crates/gradiance-interaction/src/input.rs` — `KEY_OPS`, the id link.
- `docs/scripting.md` — the verb recipe.
- `crates/gradiance-domain/src/joint.rs:73` — `MotorConfig`.

## Facts

- Every action lowers to an existing intent: `delete-joint` (many),
  `spawn-joint` (world pin), `spawn-node` (thruster), `property-edit`
  (shape), joint property edit (motor). No new command type is needed.
- A multi-intent action must still be one undo step. *Unverified* that
  `OpOutput` batches several intents into one step; check before building.

## Approach

- Operations: `body.loosen`, `body.pin_centre`, `body.thruster_centre`,
  `body.to_circle`, `joint.brake` (motor on, target 0, cap kept).
- Context menu rows invoke them by id (`InvokeOp`), never the intent.
- Keep the rows as one table of operation ids by selection kind (as
  `ui::pie::TOOLS_PIE` is): the contextual task bar (B-2, 2026-11-21 plan)
  renders the same table, so add no per-row UI logic.

## Out of scope

New joint kinds. Gesture input.

## Done when

- Undo matrix covers all five operations.
- Test: `to_circle` keeps the area within 0.5 %.
- fmt, clippy, tests, doc gate green; `docs/ops.md` regenerated.

## Outcome
