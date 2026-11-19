# Headless rollout runner
Phase R · roadmap § "Run & read surface" · status: in review

Written by the planning thread, 2026-11-19. Docs only.

## Goal

`gradiance run scene.ron [--script s.scm]… --steps N [--csv out.csv]
[--signals a,b]` loads a scene, runs the scripts, advances exactly `N`
fixed physics steps with no window, writes the CSV and exits. A script
error or a failed load gives a non-zero exit code.

This is the "deterministic headless rollout harness" from the solver
design sessions. It unblocks parameter sweeps from a shell loop, system
identification for `gradiance-optimize`, and Phase 6 replay tests.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `main.rs` already parses `--script`, `--remote`, `--review`. The client
  doorways (`ops`, `op`, `mcp`) live in `gradiance-script::client`.
- U-6 decided that `op`/`mcp` are clients of a running app, not hosts
  (brief U-6, Decision 1). `run` is different: it **is** a host, because
  it must own the clock.
- R-1 provides `sim.step`, `scene.load` and `signal.export`. `run` is a
  composition of them, not new logic.

## Unverified (the worker checks these first)

- That the app starts with no window: winit, render, egui and the
  screenshot plugin must be left out or made inert. `tests/it` already
  has `headless_app` / `paused_app` (the full editor stack with no
  window); start from that harness and move what `run` needs into the
  root package.
- That two runs of the same scene give the same CSV bit for bit. Avian
  with a fixed timestep should be deterministic on one thread. The
  workspace takes `avian2d` with default features (only `serialize` is
  added); check whether those include `parallel`, which can break that. If it is not deterministic, record
  it in the Outcome and make the test compare with a tolerance.

## Out of scope

Sweeps and optimizer loops in Rust (a shell loop over `run` covers them
for now); MCP access to `run`.

## Done when

- `gradiance run` works in CI with no display, and an integration test
  runs a falling-box scene twice and compares the CSVs.
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-r2`.

**Landed:**

- `src/run.rs` (`gradiance::run`): `parse_args`, `rollout`, `main` (the
  binary's doorway, called before the client doorways in `main.rs`) and
  `headless_app_with`, the windowless builder moved out of
  `tests/it/harness` (the harness now calls it). `rollout` composes the R-1
  operations: `scene.load`, each script through `ScriptInputs` with a reply
  slot (an error stops the run), the steps a script owes, then `sim.step`
  in chunks of at most 216000, then `signals_csv` (what `signal.export`
  writes). Each update advances virtual time by exactly the fixed timestep,
  so one update is one step and one recorded row. The app starts paused
  (`insert_state`), so nothing records before the steps. Exit codes: 0, 1
  (load, script, signal, write, stall), 2 (usage); `gradiance run --help`.
- `gradiance-signal`: `SignalBus::set_history_cap` (default 600); the run
  sets `usize::MAX`, so a rollout keeps every row.
- `gradiance-physics::TransportSet`: `apply_sim_transport` is in it and the
  signal recorders run after it. Before, the frame a `sim.step` landed
  recorded `t` but not body sensors (the sensor publisher ran before the
  transport): a fix for the app too, at no per-frame cost.
- Tests: `tests/it/headless_run.rs` (two 700-step runs of a falling-box
  scene with two boxes on a ground give the same CSV byte for byte, 701
  rows at `t = i/60`, the box rests at 0.5 m; stdout and default columns;
  a missing or unparsable scene, a script error, a missing script and an
  unknown signal give exit code 1; usage errors give 2); a signal unit test
  for the lifted cap.
- Docs: `docs/scripting.md` § "Headless rollout", CLAUDE.md build line.
  No operation changed, so `docs/ops.md` is unchanged.

**Decisions** (`docs/review/archive.ron`, `decision: true`): `run-csv-rows`
(the start state, then one row per step: N+1 rows), `run-history-cap`,
`run-defaults` (no `--csv`: stdout; no `--signals`: every signal but `t`),
`run-one-thread` (compute pools pinned to one thread).

**Checked (the brief's unverified items):**

- No window: `MinimalPlugins` + states, transform and input, with the full
  `GradiancePlugins`, as the test harness ran. The real binary ran
  `gradiance run tests/fixtures/scenes/pendulum.ron --steps 3` with
  `DISPLAY` and `WAYLAND_DISPLAY` unset: exit 0, 4 rows. The CI need is met
  by the `it` test, which calls the same `rollout`/`main`; no workflow
  change.
- Determinism: `avian2d`'s default features include `parallel`. The
  falling-box CSV was bit-identical between runs with the run's one thread
  and also, in a one-off check, with the default pools on 4 threads. The
  test compares bytes, no tolerance.

**Not checked:** scenes with many bodies or joints for determinism across
machines (CI runs the test on its own machine only); run time of long
rollouts (the whole `headless_run` module, four 700-step runs included, takes about 6 s in the opt-1
test profile).
