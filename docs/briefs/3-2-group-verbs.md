# Group and ungroup from a script
Phase 3 · scripting P3 · "`group`/`ungroup` via list argument" (roadmap:
"Still absent: set-shaped edits (`group`/`ungroup`), which want a list
argument — … `SteelVal::ListV` makes `(group (list 0 1 2))` workable") ·
status: ready

## Goal

`(group (list 0 2 3))` groups those bodies (by index, id order, like every
other indexed verb) and `(ungroup (list 0 2))` peels their outermost group.
Each emits the same `GroupIntent` / `UngroupIntent` that Ctrl+G /
Ctrl+Shift+G emit, so a scripted grouping and a hand-made one are
indistinguishable in the save file and on the undo stack.

## Read first

- `docs/scripting.md` § "Adding a verb (contributor recipe)" and the three
  scripting rules in CLAUDE.md (a verb queues through an existing seam;
  reads never reach up; one snapshot per run).
- `crates/gradiance-command/src/intent.rs` ≈L170–190: `GroupIntent {
  targets: Vec<StableId> }`, `UngroupIntent`.
- `crates/gradiance-script/src/bridge.rs`: an existing indexed edit verb
  (e.g. `delete` or `merge`) for how an index resolves to a `StableId`
  through the run snapshot; `registry.rs` for the `OpSpec` row; ≈L1558 for
  building a `SteelVal::ListV` (the reverse direction).
- `crates/gradiance-domain/src/group.rs`: `SelectionGroup` is a stack (M14);
  ungroup peels the outer level.

## Facts (orchestrator, 2026-11-13, main `93c9a76`)

- steel's `RestArgs` is private, so true variadics are out; a list argument
  is the convention the roadmap names (`ListV`).
- Selection lives above `gradiance-script`, so these verbs are **indexed**,
  never selection-scoped (CLAUDE.md scripting rule 2).
- *Unverified:* whether `GroupIntent` with fewer than two targets is
  rejected by `GroupCommand` or makes a one-body group. Match whatever
  Ctrl+G does with one body selected.

## Approach

1. A helper that turns a `SteelVal::ListV` of numbers into indices, then
   into `StableId`s through the run snapshot. A non-list, a non-number
   element, or an out-of-range index is a visible script error (the 2-5
   pattern: `warn!` plus a failed console entry), not a silent no-op.
   Duplicates are collapsed.
2. `group` / `ungroup` verbs with `OpSpec` rows (category: edit) and docs.
3. Reads: a `(body-group i)` query is **not** in scope; if a test needs to
   observe grouping, read it in Rust from the world.

## Out of scope

Selection-scoped verbs, group naming, any UI change, list arguments for
other verbs, `docs/state.md`.

## Done when

- Integration tests: `(group (list 0 1))` then a scene capture shows a shared
  outermost group id; `(ungroup (list 0))` peels it; one undo reverts each;
  a bad argument is a script error and changes nothing.
- `docs/scripting.md` verb reference updated; roadmap P3 "Still absent:
  set-shaped edits" line marked landed.
- Gates green (workspace tests via CI if disk is tight). Outcome filled.

## Outcome

*(worker fills in)*
