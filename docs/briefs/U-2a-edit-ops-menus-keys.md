# Edit operations, menus and keys on the registry
Phase U · `docs/unified-surface-plan.md` § 3.4–3.6, § 13.4 · follows U-1
(#243) · status: open

## Goal

Every **edit** verb is one `gradiance-op` declaration. The Edit menu, the
body context menu and the key handlers invoke operation ids through
`InvokeOp`, not intents. One table links each `EditorAction` variant to an
operation id (plan § 13.4). The conformance tests of U-1 cover all of it
without new hand lists.

Split from the planned U-2 (2026-11-17, planner). U-2 as written moved 56
verbs, the menus, the key table and the deletion of the old paths in one PR.
That exceeds the coherence budget of one worker (`docs/orchestration.md`).
U-2a takes the mutation surface. U-2b takes queries, config, editor verbs,
the deletion of the old paths and the generated `docs/ops.md`.

## Read first

- `docs/briefs/U-1-operation-registry.md` § Outcome (what landed, the six
  decisions, "For U-2").
- `crates/gradiance-op/src/` (all five files, small) and
  `crates/gradiance-command/src/ops.rs` (the five operations, the filler,
  `SceneOpsPlugin`, `install`).
- `crates/gradiance-script/src/bridge.rs`: `edit_bindings` (604),
  `register_edit_verbs` (873), `register_boolean_verbs` (1253),
  `register_lifecycle_verbs` (1333), `register_group_verbs` (1412),
  `register_property_verbs` (1451), `register_joint_verbs` (1558), `bind_op`.
- `crates/gradiance-ui/src/menu.rs` (`MenuWriters`, lines 73–77, 222–248),
  `crates/gradiance-ui/src/context_menu.rs` (161–185, 396–850),
  `crates/gradiance-interaction/src/input.rs` (121–193).
- `tests/it/op_conformance.rs` (the four tests; they iterate the registry).

## Facts (planner, 2026-11-17, measured on `dc70682`)

- 22 edit verbs are still hand-written: `cut`, `spawn-circle`,
  `spawn-ground`, `spawn-box-rot`, `delete`, `redo`, `hinge`, `slider`,
  `spring`, `set-friction`, `set-restitution`, `set-density`, `set-static`,
  `scale`, `merge`, `subtract`, `intersect`, `xor`, `fillet-merge`,
  `delete-joint`, `group`, `ungroup`.
- Direct intent writes outside `command`: `menu.rs` (Redo, Delete, Group,
  Ungroup, LoadScene), `context_menu.rs` (Group, Ungroup, Merge, Boolean,
  Delete, DeleteJoint, CommitTransform, PropertyEdit), `input.rs` (Undo,
  Redo, Delete, DeleteJoint, Group, Ungroup).
- Menus and keys act on the **selection**. The selection lives in
  `gradiance-interaction`, above `command`. `Target::Selection` resolves
  only where a selection mirror exists (U-1 decision 2), and none exists.
- `EditorAction` also has non-edit variants (`TogglePause`, `SelectAll`,
  `Deselect`, `Save`, `SaveAs`, `Open`, `Snapshot`, `ToggleScaleFrame`,
  `Tool(_)`). Their operations belong to U-2b and U-3.
- Gesture tools (drag, joint edit, pack) and inspector fields write intents
  that carry old values from one gesture. They are not doorways of a named
  operation. They stay as they are in U-2a.

## Approach

1. **Selection mirror.** `gradiance-interaction` publishes a read-only
   selection mirror into `OpContext` through its own filler (layer rule:
   each layer publishes its own mirror). The Lisp doorway must not resolve
   `"selection"` (`CLAUDE.md` § Scripting rule 2): give the script run a
   context without that mirror, or reject `Target::Selection` there with a
   visible error. Pin the choice with a test.
2. **Operations.** Declare the 22 edit verbs in the layer that owns their
   intent (mostly `gradiance-command/src/ops.rs`; split the file by family
   when it passes about 600 lines). Today's verb names and argument order
   stay valid. Set-shaped params use `Vec<Target>`. Delete their closures,
   `name::` constants, catalog rows and `edit_bindings` rows.
3. **Derive or not.** Decide with 27 rows of evidence (U-1 decision 6). A
   `macro_rules!` is fine. A proc-macro crate is a new DAG row; take it only
   if it removes a real second description, and record the reason.
4. **Menus and keys.** Menu rows, context-menu rows and key handlers that
   name an edit operation write `InvokeOp`. One table in `input.rs` (or
   beside it) maps each edit `EditorAction` variant to an operation id; the
   handler looks up the id and invokes it. `EditorAction` and the leafwing
   `InputMap` do not change. Context-menu rows that commit a gesture-style
   change with old values (`PropertyEditIntent`, `CommitTransformIntent`)
   may stay if no operation fits; list each in the Outcome.
5. **Conformance.** Extend the existing tests, do not add hand lists:
   every key-table entry names a registered id; the menu scanner also scans
   `context_menu.rs` and `input.rs` for the migrated intents; doorway
   equivalence includes the menu/key doorway with a fixture selection.

## Out of scope

Query, config and editor verbs; deleting `OpSpec`, `EditBinding` or the
remote Scheme path; `docs/ops.md` (all U-2b). Gesture tools and inspector
fields (U-3 widget registry). Any key change (owner: keys out of scope).
`set-color` and group verbs beyond `group`/`ungroup` (4b-1). New
user-visible behaviour, except visible errors on bad input.

## Done when

- The 22 edit verbs are registry operations; their hand-written code is gone.
- No `MessageWriter` of a migrated intent remains in `menu.rs`,
  `context_menu.rs` or `input.rs`, except the rows the Outcome lists.
- The key table exists and a conformance test checks it.
- A script cannot act on the selection; a test pins it.
- fmt, clippy, doc local; `cargo test --workspace` in CI; CI green.
- The Outcome lists what U-2b inherits.

## Outcome

(open)
