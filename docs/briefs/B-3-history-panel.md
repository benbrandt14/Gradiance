# History panel
Phase B · roadmap § "Unified plan" · status: ready (native; any gate after Phase T)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 21). Docs only.

## Goal

A History panel lists the undo timeline (each step's label: `spawn-body`,
`reshape`, `run`, `settings` …) with the cursor marked; clicking a row moves
the cursor there (undo or redo by as many steps); rows after the cursor show
greyed. Runs show their duration.

## Read first

- `docs/interaction-feel.md` — row **Blender**.
- `crates/gradiance-command/` — `CommandStack` (a snapshot timeline with a cursor; `docs/roadmap.md` § Adoption audit A).
- VectorCraft `crates/ui-egui/src/panels/history.rs` at `9fff48d`.

## Approach

- A read-only mirror of the stack's labels published by `gradiance-command`
  (UI never reads the stack directly); clicking emits a new
  `history.goto n` operation that lowers to n undo/redo intents in one run.
- `PanelToggle` + View menu row.

## Out of scope

Branching history.

## Done when

- Behaviour test: `history.goto` to step 2 of 5 equals two undos then
  `scene.dump` equal; redo back equal.
- UI test that the panel lists the labels.
- Gates green.

## Outcome
