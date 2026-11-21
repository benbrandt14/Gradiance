# Contextual task bar
Phase B · roadmap § "Unified plan" · status: ready (after S-6; port after B-1, or native before it)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 20). Docs only.

## Goal

A small bar floats below (or above) the selection with the operations that
fit it: for bodies, the S-6 quick actions plus mirror, merge, booleans,
make static; for one path body, close/open and corners; for a joint, its
motor and limits toggles. The rows come from one table of operation ids
(like `TOOLS_PIE`), filtered by each operation's selection params; the
person can turn rows off (a "Bar actions" menu), which is workstation
config. The bar moves out of the way of the selection and of the pointer.

## Read first

- `docs/interaction-feel.md` — row **Illustrator** (bar by the selection) with Blender's rule (rows are operations).
- `crates/gradiance-ui/src/pie.rs` — a table of operation ids rendered as a control surface.
- `docs/briefs/S-6-context-quick-actions.md` — the quick actions are the first rows.
- VectorCraft `crates/ui-egui/src/canvas/task_bar.rs` at `9fff48d`.

## Approach

- Feel: **Illustrator/Blender**. `ui::task_bar` with `TASK_BAR` (op ids by
  selection kind); invokes through `InvokeOp`; no logic in UI.
- `View ▸ Task bar` (`PanelToggle`); the hidden rows live in workstation
  config with the workspaces (U-10).

## Out of scope

Tool-local bars (H-1 has its own).

## Done when

- A UI test (kittest) that the bar for a body selection lists exactly the
  table's body rows and that a click invokes the op (one undo step).
- Gates green; screenshot.

## Outcome
