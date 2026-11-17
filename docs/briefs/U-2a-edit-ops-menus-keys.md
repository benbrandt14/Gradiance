# Edit operations, menus and keys on the registry
Phase U · `docs/unified-surface-plan.md` § 3.4–3.6, § 13.4 · follows U-1
(#243) · status: done (PR: see docs/state.md)

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

**What landed.**

- **27 registry operations** (the 22 edit verbs plus U-1's five), in
  `crates/gradiance-command/src/ops/`, split by family: `spawn.rs`
  (`body.spawn_box`, `body.spawn_box_rot`, `body.spawn_circle`,
  `body.spawn_ground`, `body.cut`), `body.rs` (`body.place`,
  `body.delete`, `body.scale`, `body.set_friction`, `body.set_restitution`,
  `body.set_density`, `body.set_static`), `sets.rs` (`body.merge`,
  `body.fillet_merge`, `body.subtract`, `body.intersect`, `body.xor`,
  `group.create`, `group.ungroup`), `joint.rs` (`joint.hinge`,
  `joint.slider`, `joint.spring`, `joint.delete`), `scene.rs`
  (`edit.undo`, `edit.redo`, `body.count`, `sim.set`). Lisp names and
  argument order are unchanged. New ids come from `ctx.new_id()` (bodies
  and joints).
- **Hand-written code gone:** `register_edit_verbs`,
  `register_boolean_verbs`, `register_lifecycle_verbs`,
  `register_group_verbs`, `register_joint_verbs` + `spawn_joint`, the write
  half of `register_property_verbs`, `body_ids_arg`, `emit`,
  `body_record`, `ground_record`, the 22 `name::` constants and catalog
  rows, the `edit_bindings` rows, `Queued::Intent` and
  `IntentDispatch::apply`. The op queue now holds only registry outputs.
- **Selection mirror.** `gradiance_op::SelectionMirror { objects, joint }`
  (ids only, so it lives in `gradiance-op`); `gradiance-interaction`
  publishes it through its own filler (`selection::fill_selection_mirror`).
  New DAG edge `interaction → op` (row in `tests/boundaries.rs`).
  `InteractionSet` now runs before `OpInvokeSet`, so a key edit is a
  command in the same frame.
- **Scripts cannot act on the selection.** `run_scripts` builds its context
  and withdraws the mirror (`OpContext::remove`). `"selection"` there is the
  visible error `OpError::NoSelection`: `"selection" is not available here:
  name objects by index or id`. Pinned by
  `every_key_row_acts_on_the_selection_and_a_script_cannot` (with a real
  selection present) and by unit tests in `ops/mod.rs` and `context.rs`.
- **Key link table:** `interaction::input::KEY_OPS` (`EditorAction`, id,
  params JSON), six rows: Undo, Redo, DeleteSelection (two rows:
  `joint.delete` and `body.delete`, both on `"selection"`; joint and body
  selection are exclusive, so exactly one acts, as the old joint priority
  did), Group, Ungroup. `apply_shortcuts` iterates it; Group yields to
  Ungroup as before. `EditorAction` and the `InputMap` are unchanged.
- **Menus:** Edit ▸ Redo / Delete selection / Group / Ungroup and the
  context menu's Group / Ungroup write `key_invocations(action)`, so a key
  and its menu row cannot differ. The context menu's Merge, Merge with
  fillet, Subtract, Intersect, Xor invoke `body.*` with
  `{"a": "selection", "b": "selection"}`; Delete joint and Delete node
  invoke `joint.delete` / `body.delete` with the clicked object's id.
- `gradiance-op` additions: `SelectionMirror`, `OpContext::remove`,
  `OpContext::selection`, `OpError::NoSelection`, `InvokeOp::with`,
  `InvokeOp::parse`, re-exports `json!`/`Value` (so the UI and the
  interaction layer need no `serde_json` dependency).
  `SceneSnapshot` gained `nodes`, `get(id)` and `BodySnapshot::pose`.
- **Conformance** (`tests/it/op_conformance.rs`, 5 tests, all iterate the
  registry or `KEY_OPS`):
  - fixture now has fixed ids (index i = i-th spawn): three overlapping
    circles (every boolean has an effect), two grouped bodies, a world pin,
    one step on the redo stack;
  - test 1b (new): every key row names a registered id and parses; with a
    fixture selection (bodies 0 and 1, then joint 0) the key/menu
    invocation and the remote `id` form lower to the same output, and the
    output names exactly the selected objects; the Lisp form of the same
    params fails visibly;
  - test 4 now scans all of `gradiance-ui` and `input.rs` for id literals
    in registered namespaces, and checks that `menu.rs`, `context_menu.rs`
    and `input.rs` name no intent a registry edit writes. The migrated set
    is read from the operations' example outputs, not a hand list;
  - test 6: a history operation (undo, redo) moves exactly one step
    between the stacks.

**Decisions** (non-interactive, owner ruling 2026-11-17):

1. *Selection semantics.* `"selection"` names every selected body where
   the intent holds a set (delete, property edits, booleans, groups) and
   the primary (first-selected) body where it holds one (place, scale,
   joint ends). `joint.delete` resolves it to the selected joint.
   `body.delete` with `"selection"` or an id also deletes behaviour nodes
   (the key used to, and the node menu needs it), so the snapshot lists
   node ids.
2. *Booleans keep `(merge a b)`.* The menu acts on N bodies by passing
   `"selection"` for both; the targets are `a`'s bodies then `b`'s,
   distinct, so the first-selected body still hosts the result.
3. *Bad input.* Unchanged quiet no-ops: an index that names nothing in a
   scalar field, a non-positive scale factor, a boolean of one body,
   `group` with fewer than two bodies, setting a property to its current
   value. Visible errors (new for some verbs, as in U-1): a wrong-type or
   non-finite argument to any of the 22 verbs, a list member that names
   no body (`group`, `ungroup`, as before), a bad fillet radius (as
   before). A joint verb whose `a` names nothing still returns `""`.
4. *Ids.* `body.*` (incl. `body.cut`, the booleans), `joint.*`,
   `group.create` / `group.ungroup` (4b-1 extends `group.*`), `edit.redo`.
5. *No derive macro, no proc-macro.* 27 `impl Operation` blocks; the
   repetition is `menu: None, chord: None` (two lines each). A
   `macro_rules!` would hide the declaration a reader greps for; a
   proc-macro would remove only the doc-comment / `doc:` duplication, not
   worth a new DAG row. Revisit if U-2b's 34 rows show more.
6. *The key table holds params as JSON text* (a `const` table cannot hold a
   `serde_json::Value`); `KeyOp::invoke` parses, and the conformance test
   keeps every row parsing.
7. `IntentDispatch`, `EditBinding` and `edit_bindings()` stay (now empty)
   for U-2b to delete, as the brief scopes.

**Rows left on intents** (listed in `DIRECT_INTENT_ROWS` in the test):

- `context_menu.rs` `PropertyEditIntent`: the node-kind editor, the joint
  "connected bodies collide" checkbox and the embedded inspector sections.
  Inspector fields with old values; U-3's widget registry.
- `context_menu.rs` `CommitTransformIntent`: Align & distribute (one
  transform commit computed from bounds; no operation fits).
- `menu.rs` `LoadSceneIntent` (File ▸ New): not an edit verb of this brief;
  no operation writes it, so the scanner does not flag it.

**What U-2b inherits.**

- Delete `IntentDispatch`, `EditBinding`, `edit_bindings()` and their test
  `tests/it/registry_validation.rs::every_edit_op_maps_to_a_registered_intent`
  / `every_edit_intent_resolves_in_the_type_registry` (both now trivially
  pass over an empty table).
- The query, config and editor verbs; File ▸ New (`LoadSceneIntent`).
- Remote error text repeats the id: `group.create: group.create: invalid
  type …` (`OpError::Decode` already names the op and the remote doorway
  adds it again; pre-existing from U-1).
- A key row that fails to parse is only a `warn!` at run time; the
  conformance test is the guard.
- The remote `id` form builds a full context, so `"selection"` resolves
  there (the live app's selection), like a menu. U-6 should decide whether
  CLI/MCP keep that.

**A failing case (shown 2026-11-17).** With a side door planted in
`menu.rs` (a `MessageWriter<DeleteIntent>` field) and the Group key row
broken (`{"targets": "selection"}`, not a list), `cargo test --test it
op_conformance` failed twice:
`every_invoked_id_is_registered_and_no_side_door_remains`: `menu.rs names
\`DeleteIntent\`: invoke the operation that writes it`, and
`every_key_row_acts_on_the_selection_and_a_script_cannot`: `group.create:
Some(Err(BrpError { code: -32602, message: "group.create: group.create:
invalid type: string \"selection\", expected a sequence" }))`. The other
three passed. Both breaks were reverted before the commit.

**Checked locally** (rustc 1.100.0): fmt, clippy `-D warnings`, doc
`-D warnings`; `cargo test --test it` (whole umbrella binary: 326 passed);
`op_conformance` alone after the revert (5 passed); `cargo test -p
gradiance-op` (14 passed), `-p gradiance-command` (7), `-p
gradiance-interaction` (36), `-p gradiance-script` (43);
`cargo test --test boundaries` (8 passed).

**Not checked locally:** `cargo test --workspace` (CI); the app was not
run, so menu and key behaviour is checked by the tests only (no
screenshot; the harness cannot send keys).
