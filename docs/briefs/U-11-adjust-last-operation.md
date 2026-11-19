# Adjust Last Operation
Phase U · `docs/unified-surface-plan.md` § 7.2 (row "Adjust Last
Operation panel"), § 10 row 11 · status: ready

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

*(worker fills in)*
