# Queries, config and editor verbs on the registry; old paths deleted
Phase U · `docs/unified-surface-plan.md` § 3.4–3.6 · follows U-2a ·
status: open (start after U-2a merges)

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

(open)
