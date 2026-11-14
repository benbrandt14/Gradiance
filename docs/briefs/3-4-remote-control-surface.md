# The external control surface (BRP)
Phase 3 · adoption audit rows 7/17 · "`reflect_functions` + an app-level
`bevy_remote` (BRP) seam exposing reads and `gradiance.op.*`. This is the
MCP door — design it as the *external* control surface, not a debug toy."
· status: done

## Goal

A process outside the app can drive Gradiance over Bevy's remote protocol
(JSON-RPC over HTTP) through **the same operation registry scripts use**:
- list ops with their signatures and docs;
- invoke an op (`gradiance.op.<name>` with positional args); the call goes
  through the same intents and seams as a script or tool;
- run read queries.

Raw ECS mutation (`world.insert`/`mutate`/`despawn`/`spawn`) is **not**
exposed: writes stay seam-mediated. The surface is off by default and
turned on by a CLI flag or a cargo feature (your call, documented).

## Read first

- `docs/roadmap.md` § External-crate adoption audit, C, the
  "reflect_functions + curated BRP" bullet; `docs/orchestration.md` § Phase 3
  and § Future intent 1 (an external UI and MCP tooling will bind here).
- `crates/gradiance-script/src/registry.rs`: `OperationCatalog`, `OpSpec`
  (name, signature, doc, governance category), `OperationRegistry`.
- `crates/gradiance-script/src/bridge.rs`: `run_scripts` (the one
  ECS-touching doorway), `ScriptInputs`, the per-run `SceneView` snapshot.
- Bevy 0.19 `bevy_remote` (`~/.cargo/registry/src/*/bevy_remote-0.19.0/`
  once fetched): `RemotePlugin::with_method`, `RemoteHttpPlugin`, the
  built-in `world.*` methods, and how to leave them out or restrict them.
  `docs/bevy19-notes.md` first.
- `tests/boundaries.rs`: the crate DAG and dependency pins.

## Facts (orchestrator, 2026-11-13, main `93c9a76`)

- `bevy = "=0.19.0"` with default features; `bevy_remote` and
  `reflect_functions` are optional features (bevy's Cargo.toml ≈L2689,
  ≈L2833). `bevy_remote` is not in the local registry yet, so enabling it
  changes `Cargo.lock`. That is allowed here as a **deliberate, reviewed**
  lockfile change; say so in the PR. The exact-pin line in
  `tests/boundaries.rs` must still pass.
- Every verb already has an `OpSpec`, and scripts invoke verbs by submitting
  source to `ScriptInputs`. A remote `op` call can therefore be lowered to
  the same path: build a script call from the method name and JSON args, or
  call the verb's Rust function directly if the registry exposes one.
  Pick one and record why. Either way, one remote call = one script run =
  one undo batch, preserving the one-snapshot-per-run rule.
- *Unverified:* whether `bevy_remote`'s built-in methods can be omitted or
  must be filtered; whether `RemoteHttpPlugin` binds only to localhost by
  default (it must here).
- The audit row also names `reflect_functions` plus "a CI test that every op
  resolves in `FunctionRegistry`". Do it only if it falls out naturally from
  the design above. If it is separable, say so in the Outcome and leave it
  out.

## Approach

1. A small app-level plugin (crate placement is part of the design: likely
   `gradiance-script`, since it owns the registry, or the root app shell;
   don't add a new crate edge without a `boundaries.rs` row and a reason):
   - methods `gradiance.ops` (list), `gradiance.op.<name>` or a single
     `gradiance.op` with `{name, args}` (pick one and document it), and
     `gradiance.query` for the read verbs;
   - results are JSON. Script errors come back as JSON-RPC errors carrying
     the same message the console shows.
2. Built-in `world.*` mutation methods are disabled. Read-only `world.*`
   methods stay only if harmless; list which.
3. Off by default; localhost only; documented in `docs/scripting.md`
   (a new "Remote control" section with a `curl` example).

Invariants: 1–2 (all writes through intents/settings seams via the
registry); steel stays confined to `gradiance-script`; the two-tier rule
(remote calls are cold-path, handled like REPL lines).

## Out of scope

An MCP server, authentication beyond localhost binding, streaming or
subscriptions, new verbs, UI, `docs/state.md`.

## Done when

- Integration tests drive the plugin in-process (no network): list ops; call
  `spawn-box` and see one body plus one undo step; a bad op name or arg is a
  JSON-RPC error with the console's message; a `world.insert`-style call is
  rejected.
- A manual check with the real binary (`--remote` or the feature): start it
  under `tools/screenshot/session.sh`, `curl` a `spawn-box`, grab a
  screenshot showing the box. Describe it in the PR, keep it out of git, and
  report the path.
- `docs/scripting.md` § Remote control; roadmap audit C row updated. Gates
  green. Outcome filled.

## Outcome

**Landed (PR #229).** `crates/gradiance-script/src/remote.rs`:
`RemoteControlPlugin` adds Bevy's `RemotePlugin` (+ `RemoteHttpPlugin` on
`127.0.0.1`) with three methods over the op registry. Placed in
`gradiance-script` because it owns the registry and the doorway; no new
crate edge.

- **Method shape:** one `gradiance.op` with `{"name", "args"}` rather than
  `gradiance.op.<name>`: BRP's method table is fixed at plugin build, while
  the catalog is data that will grow (user tools from `.scm`); one method
  keeps `rpc.discover` small and the op names in one place.
  `gradiance.query` is the same call restricted to `query` ops (a read-only
  client needs only it); `gradiance.ops` lists the catalog.
- **A call becomes a run:** the name must be a catalog entry; each JSON arg
  is rendered as a Scheme literal (`null`/objects refused), and the call
  `(spawn-box 0 1 1.6 1.2)` is submitted with `ScriptInputs::submit_with_reply`
  and run through `run_scripts` — lowering to the script path because verbs
  exist only as steel builtins (a direct Rust path would duplicate their
  coercion and rollback rules). `ScriptRun` gained an optional `reply` slot
  (`RunOutcome = Result<serde_json::Value, String>`). The intents wait for
  the ordinary dispatcher (next frame's `Update`). Same budget,
  all-or-nothing, one undo batch, one console log line.
- **Errors:** `-32602` for unknown op / bad params / an arg with no Scheme
  form / a non-query op on `gradiance.query`; `-32000` for a failed run,
  `message` = the console text verbatim, `data.source` = the run's source.
- **Built-ins:** `RemotePlugin` has no empty constructor, so `finish()`
  rebuilds `RemoteMethods` keeping only ours plus the reflection reads
  `rpc.discover`, `world.query`, `world.get_components`,
  `world.list_components`, `world.get_resources`, `world.list_resources`,
  `registry.schema` (reads are total). Every `world.*` write is removed,
  **including `world.trigger_event` and `world.write_message`** (the roadmap
  row had kept `trigger_event`; both are mutation paths around the registry —
  `write_message` could even emit raw intents unchecked). `+watch` streams
  (out of scope) and `schedule.*` also go. With a renderer Bevy serves the
  render world on 15703; that table is emptied.
- **On:** `gradiance --remote [port]` (default 15702). Compiled always
  (bevy's `bevy_remote` feature, enabled in `gradiance-script`'s manifest):
  a cargo feature would leave the code uncompiled in CI.
- **Lockfile:** deliberate, additive: `bevy_remote` + `hyper`, `http`,
  `http-body(-util)`, `httpdate`, `smol-hyper`, a featureless `tokio`
  (hyper's trait dep), and a direct `serde_json` dep (already locked). No
  existing version changed. Enabling the feature also turns on
  `bevy_dev_tools`/`serialize` paths in bevy, so the first build after this
  recompiles much of bevy.
- **`reflect_functions`:** separable and left out; the design did not need it.

Learned: BRP instant handlers are exclusive systems run in `RemoteLast`
(after `Last`). A first version also called `dispatch_intents` there, for
read-your-writes within the same frame; it was dropped so the handler adds
no second dispatch site and a remote run's commands land exactly when a
console line's do (next `Update`). The next call, a frame later, still reads
the write (`a_remote_spawn_simulates_like_a_console_spawn` and the curl
session both show it). Two calls are two snapshots; atomic multi-step edits
go through a `register-action` helper.

Manual-check trap: at start-up one script unit is 100 screen px (the view
spans about 12.8 x 7 units); a first curl with `scripting.md`'s
pixel-looking sizes (`spawn-box 0 0 160 160`) put the box far off-screen.
The `scripting.md` examples (`spawn-ground 0 -200 0`, 20-unit boxes) may be
stale for the same reason; not touched here (out of scope).
