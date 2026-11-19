# Headless rollout runner
Phase R · roadmap § "Run & read surface" · status: planned (after R-1)

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

(empty)
