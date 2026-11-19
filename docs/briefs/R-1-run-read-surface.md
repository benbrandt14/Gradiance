# Run and read a scene from any doorway
Phase R (run & read surface) · roadmap § "Run & read surface" ·
status: in review (after Phase 5, before U-9)

Written by the planning thread, 2026-11-19, from the owner's ranked
brainstorm ("their existing order is a good priority"). Docs only: no code
in this PR.

## Goal

A script, the CLI or an MCP client can run an experiment end to end with
operations only: load a scene, play or step it, read what happened, save
it. Today a client can edit a scene but cannot do any of that. The
screenshot harness sends `ui.key "Space"` and waits for wall-clock time.

Four operation families, in this order (owner priority):

1. **Sim transport:** `sim.play`, `sim.pause`, `sim.step n`, `sim.time`.
2. **`scene.dump`:** the full `SceneRecord` as JSON, so a client reads
   joints, anchors, groups and labels without screenshots.
3. **Scene files:** `scene.save path`, `scene.load path`.
4. **Signal data out:** `signal.history name` (a list of `[t v]`) and
   `signal.export path names` (CSV).

## Facts (planning thread, 2026-11-19, on `92ab302`)

- No registered operation plays, pauses, steps, saves, loads or exports
  (`docs/ops.md`). `scene.new` is the only `scene.*` operation.
- Play/pause is `GameState` (`gradiance-core::states`). Only the
  `TogglePause` key (`interaction::input`, ≈ line 239) and
  `command::dispatch` (≈ line 128, forces `Paused`) change it.
  `physics` pauses `Time<Physics>` on `OnEnter(GameState::Paused)`.
- `OpOutput::message` queues any message for its existing drain, so a
  transport request needs no new output kind.
- `persist` already drains `SaveSceneRequest { path: Option<PathBuf> }`
  and `LoadSceneRequest { path }` (`gradiance-persist/src/lib.rs`).
  `persist` sits above `command` and below `script`: the `scene.save` /
  `scene.load` operations register **in `gradiance-persist`** (it reaches
  `gradiance-op` through `command`). The script layer must not depend on
  `persist` for this.
- `SignalBus` keeps `value`, `history` and `times` per `BusEntry`
  (`gradiance-signal/src/lib.rs` ≈ line 65); recording pauses with the
  sim.
- `ui.screenshot` already writes to a caller-given path over the remote
  port. That is the precedent for `scene.save` and `signal.export`.

## Decisions (planning thread, best estimate, owner ruling 2026-11-17)

1. **Order in the plan:** after Phase 5 (5-4, 5-5), before U-9. U-9..U-11
   are projections of a complete registry, so they gain from these
   operations. Phase 6 replay tests use R-1 and R-2.
2. **`sim.step n` returns at once,** like `ui.drag`. The client reads
   `sim.time` (physics clock, seconds) to know when the steps are done.
   A blocking call would hold the remote request across frames.
3. **`sim.step` pauses first** if the sim plays, then advances exactly `n`
   fixed steps of `Time<Physics>`. Steps are not commands and are not
   undone (rule 5: sim state is derived).
4. **Transport requests are a plain request message** drained in
   `interaction` next to the `TogglePause` key, so the key and the
   operation share one path. Starting a run is not a world mutation (the
   same reasoning as `StartPackRequest`).
5. **`scene.dump` serializes the record that `scene` already builds for a
   save.** No second format: JSON is the serde projection of the same
   `SceneRecord` (CLAUDE.md "single-format RON" covers what goes on disk;
   a query reply is not on disk).
6. **CSV is written by hand** (`t,name1,name2,…`, one row per sample time
   of the first signal). No new dependency.
7. **Paths are not confined** in this brief. A remote client can already
   write any path through `ui.screenshot`. Confining all path-taking
   operations to one directory is a separate, later brief if wanted.

## Approach

One worker, one PR. Land the families in the order above, each with its
`OpMeta` example (the conformance test decodes it) and one integration
test in `tests/it/` (play → step 60 → `sim.time` = 1 s at 60 Hz; dump →
reload → dump equal; export → parse CSV → row count).

## Out of scope

- The headless runner (R-2).
- Insert-from-file and copy/paste (R-3).
- Packing as an operation (R-4).
- Session macro recording and a ruler tool (rejected by the owner,
  2026-11-19; see the roadmap section).

## Done when

- The four families are in `docs/ops.md` (generated) and reachable from
  Lisp, `gradiance op`, MCP and, where a menu row fits, the menu.
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-r1`.

**Landed (nine operations, all in `docs/ops.md`):**

- Transport, `crates/gradiance-interaction/src/transport.rs`: `sim.play`,
  `sim.pause`, `sim.step` (editor seam, no undo) and `sim.time` (query).
  All write one `SimTransport` request; `apply_sim_transport` (Update,
  after `OpInvokeSet` and `InteractionSet`) is the only system that turns
  it into a `GameState` change. The Space key writes `Toggle`, the
  toolbar's Play/Pause button invokes `sim.play` / `sim.pause`.
- Steps, `crates/gradiance-physics/src/lib.rs`: `SimStep` holds the owed
  steps; `advance_owed_step` (FixedFirst) advances the **paused**
  `Time<Physics>` by one fixed timestep times the sim speed per fixed tick,
  and avian steps it (its `run_physics_schedule` runs any non-zero delta of
  a paused clock). `sim_running` (Playing, or stepping) now gates field
  forces, plane friction, joint breaking, signal recording
  (`gradiance-signal`) and tracer trails (`gradiance-render`), so a stepped
  span behaves as the same span played. Play and pause drop owed steps; a
  second `sim.step` adds to them; n is 1..216000.
- Scene files, `crates/gradiance-persist/src/ops.rs`: `scene.save`
  (editor), `scene.load` (edit, one `load-scene` step), `scene.dump`
  (query; `serde_json::to_value` of `SceneRecord::capture`, captured once
  per operation context as the `SceneDump` mirror). Save and load lower to
  the existing `SaveSceneRequest` / `LoadSceneRequest`; File ▸ Open/Save and
  Ctrl+O/Ctrl+S now invoke the operations with an empty path (= dialog).
  Save As (Ctrl+Shift+S) and Save snapshot keep their direct requests.
- Signals, `crates/gradiance-script/src/ops.rs`: `signal.history`
  (query, `[[t v] …]`) and `signal.export` (editor; CSV built while
  lowering from the `SignalHistories` mirror, written by
  `apply_editor_requests` as `EditorRequest::Export`).
- Tests: `tests/it/run_read.rs` (play/pause/step → `sim.time` +1 s for 60
  steps at 60 Hz, a pendulum swings; no undo step; dump → save → new →
  load → dump equal, and the JSON decodes to the captured record; stepped
  signals record and export, CSV rows = samples of `t`; the pendulum
  fixture loads). Fixture `tests/fixtures/scenes/pendulum.ron` (the
  `scene.load` example; regenerate with `UPDATE_FIXTURES=1`).
- Docs: `docs/scripting.md` (verb list and a "Run and read an experiment"
  section), `docs/ops.md` regenerated, CLAUDE.md layer line.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`sim-step-while-paused`, `persist-op-edge` (persist declares `op`
directly; one row in `tests/boundaries.rs`), `scene-file-paths` (empty
path = dialog; `.ron` / `.csv` required; load pre-reads the file so a
client hears the error), `signal-read-unknown` (history: empty list;
export: error).

**Learned:**

- The op-context filler now captures a full `SceneRecord` per context
  build. That is per invocation and per script run pass, and per frame
  only while F3 search is open. Measure before scenes get large; a cache
  keyed on the command revision would be wrong while playing (poses move).
- Inside one script run `(sim-step n)` then `(sim-time)` reads the old
  time (rule 3); a client polls `sim.time` from later calls. R-2's headless
  runner can wait on `SimStep::remaining() == 0` instead.
- Not checked: the real-window behaviour of the transport button and of
  stepping at frame rates above the fixed rate (headless tests run one
  fixed tick per frame).
