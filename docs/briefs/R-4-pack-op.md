# Packing as an operation
Phase R · roadmap § "Run & read surface" · status: planned (after R-3)

Written by the planning thread, 2026-11-19. Docs only.

## Goal

`(pack targets)` / `pack.run` starts the layout solver on the listed
bodies (indexed, id order) and commits one transform command when it
finishes, as the Optimizer panel does. A query reads the run's status and
report. The optimizer is then reachable from Lisp, the CLI and MCP.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `interaction::pack` drains `StartPackRequest` (a unit struct) and
  gathers the **selection**. Script rule 2 forbids a verb that reads the
  selection, so the request must carry targets (`Vec<StableId>` or
  indices resolved in the op's context); the panel then sends the
  selection's ids through the same field.
- `PackSession` already exposes `is_active`, `status`, `report`.
- `PackConfig` is a `Resource` + `Reflect`: a `pack.set path value`
  config operation follows the `sim.set` pattern if wanted.

## Out of scope

New solver strategies; packing during play.

## Done when

- One run from Lisp and one from the panel give the same committed
  layout for the same targets and config (test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
