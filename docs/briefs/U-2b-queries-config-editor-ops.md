# Queries, config and editor verbs on the registry; old paths deleted
Phase U · `docs/unified-surface-plan.md` § 3.4–3.6 · follows U-2a ·
status: done (PR: see docs/state.md)

## Goal

The remaining Lisp verbs are registry operations: 24 query, 1 config
(`camera-set`) and the editor verbs that `gradiance-script` or a lower layer
owns. The old description paths are deleted: the `OpSpec` string rows, the
`EditBinding` table and the remote Scheme lowering for registry names.
`docs/ops.md` is generated from the registry, and a test fails when it is
stale (plan § 3.5 test 5).

Split from the planned U-2 (2026-11-17, planner); see
`docs/briefs/U-2a-edit-ops-menus-keys.md` § Goal for the reason.

## Read first

- The U-1 and U-2a Outcomes.
- `crates/gradiance-script/src/bridge.rs`: `register_query_verbs`,
  `register_config_verbs`, `register_editor_verbs`, `register_action_reads`,
  `register_tool_verbs`, `register_signal_verbs`, `register_meta_verbs`.
- `crates/gradiance-script/src/registry.rs` (`OpSpec`, `OperationCatalog`),
  `crates/gradiance-script/src/remote.rs`.

## Facts (planner, 2026-11-17, measured on `dc70682`)

- Query verbs: `body-x`, `body-y`, `body-rot`, `count-at`, `nearest-at`,
  `nearest-dist`, `body-index-at`, `sim-get`, `camera-get`, `touch-count`,
  `signal-get`, `body-friction`, `body-restitution`, `body-density`,
  `body-static?`, `joint-count`, `tool-count`, `tool-x`, `tool-y`,
  `action-x`, `action-y`, `panel-open?`, `ops`, `describe` (verify the
  count of 24 against `registry.rs`; it differs by how `ops`/`describe`
  count).
- Editor verbs: `register-action`, `register-tool`, `signal-set`,
  `defparam`, `defsignal`, `label`, `panel-show`, `panel-hide`,
  `panel-toggle`. `panel-*` waits for U-3 (plan § 3.6: `ui` registers it).
- `sim-get` and `camera-get` read their own copies. They should read the
  context mirror, so one run has one settings snapshot (U-1 Outcome).
- `register-action` fills a script-side table. It needs a mirror or a
  message type from its owning layer (U-1 Outcome).

## Approach

1. Declare each verb once, in its owning layer. Query operations return
   values through `OpOutput::returning`. Editor operations that only exist
   inside the VM (`defparam`, `defsignal`, `label`) may stay built-ins if
   they have no meaning over remote; list each one kept and why.
2. `camera-set` is workstation config: no undo step (CLAUDE.md invariant 4).
3. Delete `OpSpec` string rows, `EditBinding`, `edit_bindings` and the
   Scheme lowering in `remote.rs` for every registry name. Keep the
   `{name, args}` script-run form only if a non-registry name still needs
   it; else remove it and say so in `docs/scripting.md`.
4. Generate `docs/ops.md` from the registry (a test-side generator is
   enough) and add the currency test.
5. Add the "no side doors" scanner (plan § 3.5 test 3) for `gradiance-ui`
   and `gradiance-interaction`, with an explicit allow list for gesture
   tools and inspector fields, each with a reason.

## Out of scope

`ui.*`/`panel.*` operations (U-3), the CLI and MCP server (U-6), gesture
tools and inspector fields, key changes, new user-visible behaviour.

## Done when

- Every Lisp verb is a registry operation or a listed VM built-in.
- `OpSpec` strings, `EditBinding` and the remote Scheme lowering for
  registry names are gone.
- `docs/ops.md` is generated and its currency test passes.
- The scanner test exists and fails on a planted side door (show it).
- fmt, clippy, doc local; CI green.

## Outcome

**What landed.**

- **51 registry operations** (27 from U-2a + 24 here). Every Lisp verb is a
  registry operation except the 11 listed VM built-ins. New operations, in
  their owning layers:
  - `gradiance-command` `ops/query.rs` (14 queries): `body.x`, `body.y`,
    `body.rot`, `body.friction`, `body.restitution`, `body.density`,
    `body.is_static`, `body.touch_count`, `body.count_at`,
    `body.nearest_at`, `body.nearest_dist`, `body.index_at`, `joint.count`,
    `sim.get` (reads the context's `SimSettings` mirror: one settings
    snapshot per run). `ops/scene.rs`: `scene.new` (`new-scene`, File ▸ New
    scene; `LoadSceneIntent` of `empty_scene()`, moved from `menu.rs`).
  - `gradiance-interaction` `ops.rs`: `camera.get`, `camera.set` (config,
    **no undo step**: workstation config) and the `CameraSettings` mirror
    filler (this layer inits the resource).
  - `gradiance-script` `ops.rs`: `action.register`, `tool.register`,
    `workspace.label`, `signal.set` (editor), `signal.get` (query, a
    `SignalValues` mirror filled from the bus), `signal.param` (`defparam`),
    `signal.computed` (`defsignal`). They lower to one `EditorRequest`
    message; `apply_editor_requests` (end of `run_scripts`, and a system
    after `OpInvokeSet`) moves it into its table. In a script run the
    message waits in the op queue, so a failed run discards it (all or
    nothing holds; `a_failed_run_commits_none_of_its_queued_effects`
    passes unchanged).
- **Deleted:** `IntentDispatch`, `EditBinding`, `edit_bindings()` and their
  two tests; `OpSpec` and every hand-written catalog row for a registry
  verb (the 22 query/config/editor `name::` constants, `*_specs()`); the
  hand-written query, config, editor, property-read and signal builtins
  and their queues (`SharedView`, `ConfigWrite`, the sim/camera/signal
  mirrors, the label/action/tool/signal drains); `SceneSnapshot::x/y/rot/
  physics/touch_count/body` (now unused); `InvokeOp::parse`.
- **Catalog** (`gradiance-script/src/registry.rs`): `CatalogEntry` rows =
  the registry's projection + `BUILTINS`, a const table where each built-in
  carries a `why`.
- **Remote:** the `{"name", "args"}` form maps a registry name's positional
  args through `OpEntry::named` and invokes the registry, like the `id`
  form: **no Scheme text for any registry name**. The Scheme script run is
  kept only for a VM built-in (`panel-*` still need it over remote until
  U-3). Errors name the operation once: `OpError::Decode`/`Arity` text no
  longer carries the id (U-1 decision 5); each doorway prefixes its own
  name. `group.create: group.create: …` is now `group.create: invalid
  type: …` (pinned by `a_registry_error_names_the_operation_once`).
- **Key rows:** `KeyOp::params` is a `fn() -> Value` built with `json!`, so
  a row cannot fail to parse at run time; the `warn!` is gone and
  `KeyOp::invoke` is infallible.
- **Side doors closed:** File ▸ New invokes `scene.new`; the joint
  inspector's "Delete joint" button invokes `joint.delete` (it wrote
  `DeleteJointIntent`; U-2a's scan did not cover `joint_inspector.rs`).
- **`docs/ops.md`** is generated by `ops_markdown` in
  `tests/it/op_conformance.rs`; `docs_ops_md_is_current` (plan § 3.5 test
  5) fails when stale; `UPDATE_OPS_DOC=1 cargo test --test it
  docs_ops_md_is_current` rewrites it.
- **No-side-doors scanner** (`no_intent_writer_outside_the_allow_list`,
  plan § 3.5 test 3): every `MessageWriter<…Intent>` / `Messages<…Intent>`
  in `gradiance-ui/src` and `gradiance-interaction/src` (comment lines
  skipped) must match a `SIDE_DOOR_ALLOW` row; a row that matches nothing
  fails too.
- Tests: conformance test 6 now accepts config/editor operations with no
  undo label and asserts they move no history; new integration tests
  `camera_verbs_read_and_write_camera_settings` (moved from the bridge unit
  tests: the camera verbs need interaction), `a_query_of_a_missing_body_
  reads_nan`, `a_query_of_a_missing_body_answers_null`,
  `a_registry_error_names_the_operation_once`; the remote tests follow the
  name form's new path.

**VM built-ins kept** (11, `BUILTINS`, each with its reason in the table):
`action-x`, `action-y`, `tool-count`, `tool-x`, `tool-y` (read the running
action's or tool's own invocation context, which exists only inside one VM
run); `panel-show`, `panel-hide`, `panel-toggle`, `panel-open?` (U-3:
`gradiance-ui` registers `panel.*`); `ops`, `describe` (read the VM's own
catalog; remote has `gradiance.ops`). `defparam`, `defsignal` and `label`
became operations: each has a remote meaning.

**Scanner allow list** (`SIDE_DOOR_ALLOW`, 20 rows):
- `interaction/tools/context.rs` × 11 intents (`SpawnBody`, `SpawnNode`,
  `Cut`, `SpawnJoint`, `CommitTransform`, `Scale`, `Duplicate`, `Array`,
  `Merge`, `PropertyEdit`, `SpawnRod`): gesture tools,
  `ToolCommitWriters::emit`, one commit per gesture.
- `interaction/joint_edit.rs` `PropertyEdit`: anchor/limit handle drags.
- `interaction/pack.rs` `CommitTransform`: a finished packing run.
- `ui/inspector.rs`, `ui/joint_inspector.rs`, `ui/depth_panel.rs`,
  `ui/signals.rs`, `ui/bottom_dock.rs`, `ui/context_menu.rs`
  `PropertyEdit`: inspector fields carrying the old value they show (U-3).
- `ui/context_menu.rs` `CommitTransform`: Align & distribute.

**Decisions** (non-interactive, owner ruling 2026-11-17):

1. *Query targets.* The `i` of the body reads is a `Target` (index, id,
   point; `"selection"` errors in a script), so `(body-x "<id>")` now works
   too. Point queries take `Length` x/y.
2. *NaN.* A query returns NaN for "nothing there"; `serde_json` turns it into
   `null`. The Lisp doorway maps a **query's** `null` back to NaN, so every
   documented NaN read is unchanged; remote answers `null` (it answered the
   string `"NaN"` before).
3. *Ids:* `body.*` for body reads, `joint.count`, `sim.get`, `camera.*`,
   `scene.new`, `action.register`, `tool.register`, `workspace.label`,
   `signal.{get,set,param,computed}`.
4. *`label` takes a handle string, not a `Target`:* a script labels the
   body it spawned in the same run, which the run's snapshot does not hold.
5. *Editor tables are filled through one message enum* (`EditorRequest`),
   drained in this crate. The signal layer has no edge to `gradiance-op`;
   adding one was not needed.
6. *`defparam`/`defsignal` declare `Seam::Config`, `undo: None`.* **Found:**
   `commit_settings_edits` (`gradiance-command/src/lib.rs`, `SceneSettings`)
   tracks only sim/render/lighting/scenery, not `SignalParams`/
   `ComputedSignals`/`SignalBindings`, although CLAUDE.md invariant 4 calls
   the signal-graph resources undoable scene content and
   `scene_content_eq` includes them. Conformance test 6 measured no step.
   The metadata states today's behaviour; fixing the tracker is a separate
   brief (proposal: add the three resources to `SceneSettings`, then set
   `undo: Some("settings")` on both operations; test 6 then checks it).
7. *Remote `name` form kept,* routed through the registry (above), because
   `panel-*` still needs a remote path until U-3; U-3/U-6 can drop the
   Scheme run once only meaningless built-ins remain.
8. *No derive macro* (U-2a decision 5 stands): two small `const fn`
   metadata helpers (`query`, `editor`) cover the repetition.

**Behaviour changes (bad input only, as in U-1/U-2a):** a non-number or
non-finite argument to a migrated query/editor verb is a visible error (the
reads returned NaN/0/-1, `signal-set`/`defparam` ignored it); `defsignal`
with an unparsable expression and `label` with a non-uuid handle are
visible errors (were silently dropped / a `warn!`). Over remote, a registry
verb's `name` call writes no console line and fails with `-32602` (it was a
script run, `-32000`).

**Failing cases shown (2026-11-17), then reverted:**
- Side door planted in `menu.rs` (`planted: MessageWriter<'w,
  gradiance_command::intent::DeleteIntent>`):
  `no_intent_writer_outside_the_allow_list` failed with
  `gradiance-ui/src/menu.rs declares a writer of \`DeleteIntent\`: invoke
  an operation (\`InvokeOp\`), or add an allow-list row with the reason`.
- `body.density`'s doc changed without regenerating:
  `docs_ops_md_is_current` failed with `docs/ops.md is stale (first
  difference at line 23): run \`UPDATE_OPS_DOC=1 cargo test --test it
  docs_ops_md_is_current\``, showing the on-disk and generated rows.

**What U-3 / U-6 inherit.**
- U-3: register `panel.show/hide/toggle/open` in `gradiance-ui`, delete the
  four `BUILTINS` rows and `register_panel_verbs`/`PanelRequests`'s script
  side; then the remote Scheme run serves only meaningless built-ins and
  can go. The inspector rows of `SIDE_DOOR_ALLOW` are U-3's widget-registry
  work list.
- U-6: CLI/MCP bind to the `id` form; `docs/ops.md`'s generator is the
  seed of a schema export. Query NaN is `null` on the wire. The `id` form
  still resolves `"selection"` (U-2a's open question).
- Separate brief: the signal-graph undo gap (decision 6).

**Checked locally** (rustc 1.100.0): `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`cargo test --test it` (whole umbrella binary: 330 passed);
`cargo test --test boundaries` (8 passed); `cargo test -p gradiance-op`
(14 passed); `cargo test -p <crate> --lib` for `gradiance-script` (43),
`gradiance-command` (7), `gradiance-interaction` (36).

**Not checked locally:** `cargo test --workspace` (CI); doctests; the app
was not run (File ▸ New and the joint inspector's delete button are
checked by the scanners and conformance tests only). Container note: the
cargo in this toolchain keeps one `target/debug/build/<pkg>/<hash>/` per
feature set, so `rm -rf target/debug/deps/*gradiance*` (CLAUDE.md) no
longer applies; the disk filled once and was recovered by removing
`target/debug/build/gradiance*/` and stale pre-D-1 variants.
