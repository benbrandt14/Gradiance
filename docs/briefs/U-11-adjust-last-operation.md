# Adjust Last Operation
Phase U · `docs/unified-surface-plan.md` § 7.2 (row "Adjust Last
Operation panel"), § 10 row 11 · status: in review

Written by the orchestrator, 2026-11-19 (owner ruling "scoped work").

## Goal

After an operation runs (from a menu, F3, a key, a tool commit or a
script line), a small panel at the bottom-left of the viewport shows that
operation's parameters as fields, as Blender's "Adjust Last Operation"
panel does. Changing a field **re-invokes the same operation with the new
params in place of the old one, as one undo step**: undo the last step,
apply the operation again with the edited params, and record one step.
The person can tweak a spawn's size, an array's count, a fillet's radius
or a moved body's offset without redoing the gesture.

The panel is a projection of the registry: its fields come from the
operation's params schema (`OpEntry::schema`, U-6) and the number-field
widgets (U-8, U-8b), so a new operation gets the panel with no edit here.

## Read first

- `docs/ui-design.md` (design system; number fields: edit seam =
  `precise_drag`), U-8/U-8b/U-9 Outcomes.
- `crates/gradiance-op/src/registry.rs` (`OpEntry`, `apply`, seams),
  `command::dispatch` and `CommandStack` (how a step is recorded and
  undone), `InvokeOp` (U-2a).
- `docs/unified-surface-plan.md` § 3 (one declaration per operation),
  § 7.2.

## Facts (orchestrator, 2026-11-19, main after #274)

- Every edit verb is a registry operation with serde + reflect params;
  menus, keys, F3 and scripts invoke by id (`InvokeOp`).
- Tool gestures commit one command on release (invariant 2). *Unverified:*
  whether a tool commit records which operation id and params produced
  it; if not, record the op id + params beside the command (derived, never
  persisted, never in scene content).
- Two writes to the same property inside one script run do not compose
  (one snapshot per run). The panel acts between runs, so it re-reads the
  state after the undo.

## Approach

1. A `LastOp { id, params, step }` record, set when an edit-seam
   operation commits one undo step; cleared when anything else touches the
   stack (another op, undo, redo, a load).
2. The panel: collapsible, bottom-left of the viewport, theme tokens;
   fields generated from the params schema and the current params (numbers
   with units through `unit_fields`, booleans as checkboxes, enums as a
   combo, vectors as rows). Fields with no widget yet are shown read-only.
3. Adjust = undo the step, invoke the op with edited params, record one
   step (a drag is one gesture: re-invoke on release, preview while
   dragging only if cheap).
4. Ops that cannot be re-invoked (selection-dependent ops whose selection
   changed, file ops, config/editor seams) do not show the panel; record
   the rule.
5. An op (`edit.adjust_last`, Lisp `adjust-last`) so scripts and the remote
   surface can drive it, plus a query for the current `LastOp`.
6. Tests: spawn-box then adjust its width → one undo step, the body has
   the new width, undo restores the state before the spawn; adjust after
   an undo does nothing; a tool commit is adjustable if step 1 found it
   records its op. Screenshots on lavapipe.

## Out of scope

Redo-with-repeat (Blender's Shift+R), a history panel, key bindings,
editor areas (U-10), multi-op batches.

## Done when

- The panel adjusts at least spawn, array and fillet merge in the app
  (screenshot evidence) with one undo step per adjustment.
- Tests pass; fmt, clippy, doc, boundaries green locally; CI green.
- Outcome filled; `docs/state.md` updated on the U-11 lines.

## Outcome

Worker, 2026-11-19, PR #276 (stacked on U-10, #275). The branch also holds
the U-10 follow-up commit that regenerates the four replay goldens for the
`workspaces` field (orchestrator request).

### Facts checked (the brief's "unverified")

- **A tool commit records no operation.** `ToolCommitWriters::emit` writes
  intents directly; nothing names an operation id or params. So tool
  gestures are adjustable only where the gesture commits through an
  operation: this PR routes the **array gesture** through a new
  `body.array` operation. Drawn bodies, moves, scales, joints and cuts are
  not adjustable (cut; their records carry tool defaults no operation
  takes yet).
- **There was no array operation** (the brief's "Done when" names array):
  added `body.array` (Lisp `array-copies`: targets, count, step [x y], rows,
  cross [x y]).

### Landed

- `gradiance-op::last_op` (new): `OpJournal` (each applied call of an
  edit operation with an undo label: id, params, the selection its context
  held), `LastOp`/`LastOpRecord` (the call plus the history revision after
  its step), `journal`, `invoke_as` (invoke against a given selection, or
  none) and `set_param` (dotted path; the new value takes the field's JSON
  kind, a whole number is written as an integer). `invoke` journals; the
  Lisp bridge journals each queued call (no selection).
- `CommandStack::revision` (moves with every recorded, undone or redone
  step and every clear; `clear()` keeps it moving). The dispatcher drains
  the journal: one journalled call plus one recorded command in a pass =
  the `LastOp`; any other history change retires it; `expire_last_op`
  (`Last`) retires it after a settings step or a closed run.
- `AdjustLastIntent` (drained first in `dispatch_intents`): checks the
  record is still at the stack's revision, undoes the step through the
  stack, `invoke_as` with the recorded selection; the operation's intent
  drains in the same pass. If the params do not lower, or the command
  records nothing, the step is redone and stays adjustable.
- Operations: `edit.adjust_last` (Lisp `adjust-last field value`; undo
  label none: it replaces the newest step), `edit.last_op` (query,
  `{id, params}` or null), `body.array`. `docs/ops.md` regenerated.
- `ui::adjust_panel` (new): rows from `OpEntry::schema` + the params
  (numbers/quantities = `precise_drag_unit`, counts = new
  `widgets::precise_drag_int`, booleans = checkbox, fixed number lists = a
  row, targets/text read-only), title = menu label or Lisp name in words,
  collapsible header, bottom left right of the palette above the bottom
  docks; widget names `adjust_last`, `adjust_last:header`,
  `adjust_last:<path>`. A commit invokes `edit.adjust_last` via `InvokeOp`.
- Rule (record): the panel shows when the newest step is one operation
  call's and the operation has an editable param. Not adjustable: gesture
  commits other than array, undo/redo, runs, settings edits, loads, a
  script run of several edits, ops with only targets (delete).
- Docs: `docs/ui-design.md` § 3 "Adjust Last Operation"; verb rows in
  `docs/scripting.md`; four `decision: true` entries in `archive.ron`
  (`adjust-record-rule`, `adjust-tool-gestures`, `adjust-op-shape`,
  `adjust-panel-form`).

### Tests

`tests/it/adjust_last.rs` (7): spawn-box then adjust w and h → one step,
new shape, undo removes the body; adjust after an undo does nothing; a
gesture-style intent retires the record; a script line and a fillet merge
(radius) are adjustable, a two-edit script is not; an array by count and
`step.0`; a refused adjustment keeps the step; `edit.last_op`.
`op_conformance`: the side-door row for `ArrayIntent` removed (the gesture
invokes `body.array`); a planted `LastOp` lets every doorway lower
`edit.adjust_last`'s example, and the history test checks it keeps the
depth with the operation's label. Unit tests: `set_param` (2), panel rows
(3).

### Measured / checked locally

fmt, clippy (`--workspace --all-targets -D warnings`), doc
(`-D warnings`), `--test boundaries`, `-p gradiance-op`,
`-p gradiance-command`, `-p gradiance-ui --lib`, and `--test it` for
`adjust_last op_conformance array_drag history scripting remote
ui_surfaces` (128 passed), `replay::` (4). Screenshots on lavapipe:
spawn box panel, `w` dragged 1.000 → 1.950 m in the panel (one body, new
width), array 3×2 adjusted to count 6 (14 circles), fillet merge radius
dragged 0.1 → 0.33 m, collapsed header, undo after adjust (both boxes
back, panel gone).

### Not checked

`cargo test --workspace` (CI); `ui_snapshots` (no snapshot changed: the
panel is not in a snapshot test); the panel at small window sizes; an
adjust while the simulation plays (allowed: the undo is a differential
restore, then the operation runs again).

### For later

- Enum params as a combo: the params schema names no enum variants yet
  (no operation param is an enum today).
- Other tool gestures (draw, move, scale, joints, cut) need operations
  that take the tool defaults before they can be adjusted.
- A re-invoked spawn mints a new id, so the body's colour (derived from
  its id) changes on each adjustment.
- `adjust-last` with a value the operation cannot decode (a fraction for a
  count) is accepted by the call and refused at dispatch (logged), not
  returned as the call's error.
