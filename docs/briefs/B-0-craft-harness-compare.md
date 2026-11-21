# Compare VectorCraft's control protocol and UI tests with U-4/U-5
Phase B (UI bootstrap from the Craft apps) · roadmap § "Unified plan" · status: ready (after Phase G; may run at any gate — docs only)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 23). Docs only.

## Goal

`docs/upstream-watch.md` listed VectorCraft as "not reachable" (the
`tangency-labs` path was wrong; it is `storytold/vectorcraft`, public). Do
the comparison U-4 and U-5 deferred: VectorCraft's synthetic input budgets
and hardening checklists against `ui::host::input`, and its `egui_kittest`
UI tests against `tests/it/ui_snapshots.rs`. Write a short list of what to
take, each item a brief or a "no, because".

## Read first

- `docs/unified-surface-plan.md` § 6 (porting rules) and § 13.
- `docs/briefs/U-4-input-injection.md`, `docs/briefs/U-5-snapshot-tests.md` (Outcomes).
- VectorCraft `docs/control-protocol.md` (§ Synthetic input budgets, § hardening checklists), `crates/ui-egui/src/control.rs`, `crates/ui-egui/src/tests/` at the then-current revision.

## Approach

- Read, compare, record. Update `docs/upstream-watch.md` (last reviewed
  revision) and the survey's row 23.
- Also decide which VectorCraft chrome (B-2..B-4 sources) can be vendored
  as files once on egui 0.37, and which must be native.

## Out of scope

Code.

## Done when

- `docs/upstream-watch.md` updated; a take/no-take list in this brief's
  Outcome; any new brief written.

## Outcome
