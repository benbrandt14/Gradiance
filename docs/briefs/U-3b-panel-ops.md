# Panel operations on the registry
Phase U · `docs/unified-surface-plan.md` § 10 row 6 and § 13 ("`ui`
registers `panel.*`"); cut from brief U-3 (its Outcome, "Cut") ·
status: in review

Written by the orchestrator, 2026-11-17 (owner ruling "scoped work": a
session may write the brief when the work is in the plan).

## Goal

The panel verbs become `gradiance-op` operations owned by `gradiance-ui`,
so Lisp, the remote surface and later the CLI/MCP (U-6) get them from one
declaration each, like `ui.inspect`. The four panel VM built-ins, their
steel functions and the script-side panel queue and mirror are deleted.
After that, no VM built-in makes sense outside a VM run, so the remote
surface's "built-in lowered to one script run" path is deleted too: the
remote surface becomes registry-only.

## Read first

- `crates/gradiance-ui/src/ops.rs` — the pattern to copy: `install`,
  `OpContextFillers` mirror, `OpOutput::message`, `Seam::Query` vs
  `Seam::Editor`.
- `crates/gradiance-ui/src/panels.rs` (`apply_panel_requests`,
  `publish_panel_states`, `PanelToggle`) and `menu.rs` (`Panels::named`,
  the one table).
- `crates/gradiance-script/src/registry.rs` (`BUILTINS`, `name::PANEL_*`,
  `PANELS` reason) and `bridge.rs` (`PanelRequests`, `PanelStates`,
  `register_panel_verbs`, `drain_panel_requests`-style code near :800,
  `refresh_panel_mirror`).
- `crates/gradiance-script/src/remote.rs` (`invoke`: the `call_source` /
  `ScriptInputs` fallback for built-ins).
- `docs/scripting.md` (the panel rows near :181 and :479, § near :605,
  :654) and the recipe for a new verb.

## Facts (orchestrator, 2026-11-17, on `159571d`)

- Built-ins today (`registry.rs:127`): `action-x`, `action-y`,
  `tool-count`, `tool-x`, `tool-y` (invocation context, NaN outside a run),
  `panel-show`, `panel-hide`, `panel-toggle`, `panel-open?` (reason
  `PANELS`), `ops`, `describe` (reason `META`; remote has `gradiance.ops`).
- `register_panel_verbs` (`bridge.rs:654`) queues `(name, Option<bool>)`
  into a shared `Arc<Mutex<…>>`; a system moves it into `PanelRequests`
  (`bridge.rs:417`), which `ui::panels::apply_panel_requests` drains.
  `PanelStates` (`bridge.rs:434`) is published by
  `ui::panels::publish_panel_states` and copied into the VM by
  `refresh_panel_mirror`.
- An unknown panel name today warns at apply time and is dropped. A
  registry op can do better: `lower` sees a mirror of the names and can
  return `OpError::Invalid` naming the known panels.
- The `panel-show` doc says "try (panels)": no `panels` verb exists.
- Users of the old types: `tests/it/scripting.rs` (:17, :905–:1030, :1339),
  `crates/gradiance-ui/src/lib.rs`, `menu.rs`.

## Approach

1. In `gradiance-ui` (new module or `ops.rs`): `panel.show`, `panel.hide`,
   `panel.toggle` (`Seam::Editor`, no undo; Lisp names unchanged:
   `panel-show`, `panel-hide`, `panel-toggle`; one field `name`), each
   lowering to a `PanelRequest` message the UI applies through
   `PanelToggle` as today. `panel.open` (`Seam::Query`, Lisp `panel-open?`)
   and `panel.list` (`Seam::Query`, Lisp `panels`: `[{name, open}]`) read a
   panel mirror filled by an `OpContextFillers` entry. An unknown name is
   an `OpError::Invalid` that lists the known names.
2. Move `PanelRequest`/`PanelStates` into `gradiance-ui` (or replace them
   with the message + mirror). Delete the four `BUILTINS` rows, the
   `name::PANEL_*` constants, the `PANELS` reason, `register_panel_verbs`,
   the shared panel buffers and their two systems in `bridge.rs`.
3. `remote.rs`: delete the built-in fallback (`call_source`, the
   `ScriptInputs` submit); a call that names a VM built-in is refused with
   an invalid-params error that says it exists only inside a script run.
   Update the module docs. `ScriptInputs::submit_with_reply` goes too if
   nothing else uses it.
4. Tests: move the panel cases of `tests/it/scripting.rs` to the registry
   form (Lisp still works by name); add remote cases (`panel.toggle` by id,
   `panel-open?` by name, `panels`, an unknown name refused, a built-in
   refused). Regenerate `docs/ops.md`; update `docs/scripting.md`.

Invariants: panel state is editor state (CLAUDE.md invariant 4 exception);
no new dependency edge (`gradiance-ui` already depends on `op`);
`tests/boundaries.rs` unchanged.

## Out of scope

- The View menu invoking `panel.toggle` by id (it already shares
  `Panels::named`; decide later with U-10 workspaces).
- `layout.*` operations (U-10), the inspector `PropertyEdit` rows (U-2b
  list), the run-context and meta built-ins (they stay VM built-ins).
- Any new panel, key binding or menu row.

## Done when

- The four panel verbs and `panels` work from Lisp and remote by id and
  by name; `ops.md` lists `panel.*`; the catalog has 7 built-ins.
- No `PanelRequests`/`PanelStates`/`register_panel_verbs` in
  `gradiance-script`; the remote surface has no script-run path.
- fmt, clippy, doc, `tests/boundaries.rs`, `-p gradiance-ui`,
  `-p gradiance-script` and the touched `tests/it` modules green locally;
  CI green. Outcome filled; `docs/state.md` updated.

## Outcome

Worker, 2026-11-18, on `d167c93`.

**Landed.**
- `crates/gradiance-ui/src/panel_ops.rs` (new): `panel.show`, `panel.hide`,
  `panel.toggle` (`Seam::Editor`, no undo; Lisp `panel-show`, `panel-hide`,
  `panel-toggle`, field `name`) lower to a `PanelRequest` message that
  `apply_panel_requests` applies through `PanelToggle`. `panel.open`
  (Lisp `panel-open?`) and `panel.list` (Lisp `panels`, `[{name, open}]`)
  are `Seam::Query`. All five read a `PanelMirror` that an
  `OpContextFillers` entry builds from `Panels::named` (fresh each context,
  through `run_system_cached`). A name is trimmed and lower-cased; an
  unknown one is `OpError::Invalid` listing the known names.
- `gradiance-script`: the four `BUILTINS` rows, `name::PANEL_*`, the
  `PANELS` reason, `register_panel_verbs`, the shared panel buffers,
  `drain_panel_requests`, `refresh_panel_mirror`, `PanelRequests` and
  `PanelStates` are deleted. The catalog has 7 built-ins (unit test).
- `remote.rs` is registry-only: `call_source`, `literal`, the
  `ScriptInputs` submit, `SCRIPT_ERROR` and `internal` are deleted. A VM
  built-in's name is `-32602` "… exists only inside a script run". Module
  docs updated. `ScriptInputs::submit_with_reply` stays: `src/review.rs`
  uses it.
- Docs: `docs/ops.md` regenerated; `docs/scripting.md` (verb rows,
  built-ins section, "Panels are registered ops", remote section, error
  table); `CLAUDE.md` scripting rule 2 names `PanelMirror` instead of the
  deleted `PanelStates`.

**Decisions.**
- The panel resources (and `DebugSettings`, which `Panels` borrows) and the
  apply system now install **headless** too (`panel_ops::install`, called
  from `ops::install`). Without that, every panel op fails headless, so the
  op-conformance fixture (every op's example must run) and a headless
  remote client could not use them. The render path no longer inits them.
- The mirror is read fresh per context instead of published each frame, so
  `publish_panel_states` is gone too; `apply_panel_requests` runs in
  `Update` after `OpInvokeSet` and `run_scripts`, so a request applies the
  frame it was made.
- `panel-open?` of an unknown name is now an error, not `#f` (brief:
  unknown name is `Invalid`).
- `call_source` moved into `tests/it/op_conformance.rs` (its only other
  user), with its literal-rendering test.
- `panels` in Lisp: each row is an object, which the Lisp doorway renders
  as JSON text (as for `ui-inspect`); `(length (panels))` works.

**Tests.** `tests/it/scripting.rs`: the panel cases now drive the real
panels headless (show/hide/toggle, normalised names, unknown name fails the
run and lists names, `panel-open?`/`panels` reads, a failed run applies no
panel request). `tests/it/remote.rs`: built-ins refused (replaces the
script-error test), `panel.hide`/`panel.toggle` by id, `panel-open?` by
name, `panel.open` by id, `panels`, no undo step, a panel write refused by
`gradiance.query`, an unknown name refused. Unit tests in `panel_ops`,
`registry`, `remote`.

**Checked locally (rustc 1.100.0), all exit 0:** `cargo fmt --all --
--check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`cargo test --test boundaries`; `cargo test -p gradiance-ui --lib`;
`cargo test -p gradiance-script --lib`; `cargo test -p gradiance --test
it` (the whole binary: 357 passed). **Not checked:** `cargo test
--workspace` (owner ruling: CI), the lavapipe snapshot tests (they skip
here without `GRADIANCE_REQUIRE_GPU=1`), a live `--remote` session over
HTTP, the app in a window.
