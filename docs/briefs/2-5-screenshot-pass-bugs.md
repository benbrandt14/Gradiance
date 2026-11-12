# Bugs found by the first screenshot pass
Phase 2 · substrate correctness · from the 2026-11-12 lavapipe screenshot
pass (`tools/screenshot/README.md`) · status: ready

## Goal

Three defects seen in the running app are fixed, each pinned by a headless
test, and the fix is confirmed with a screenshot taken through
`tools/screenshot/`, attached to the PR discussion and **not committed**.

1. **`sim-set` from a `--script` file has no visible effect.** A startup
   script doing `(sim-set "gravity.y" 0)` (and `0.0`) still lets bodies fall.
2. **Dropped boxes rotate on flat ground.** Two unrotated 1 m boxes spawned
   at y = 0 over a flat ground (`spawn-ground 0 -2.5 0`) land tilted about
   5° and keep rotating clockwise in lockstep (about 20° roughly 40 s of wall
   time later), balanced on a corner, drifting right. Seen in three
   separate scenes.
3. **Script-spawned bodies are grey; tool-drawn bodies get a hue.** The
   scripting bridge builds records with `Appearance::default()`, while the
   tools use the random pleasant hue. CLAUDE.md: "a scripted scene and a
   hand-built one are indistinguishable".

## Read first

- `tools/screenshot/README.md`: how to reproduce visually (lavapipe +
  Xvfb). Keyboard shortcuts don't reach the app there; click the palette.
- `crates/gradiance-script/src/bridge.rs` ≈L1630–1655 (config-queue drain,
  `let _ = write_path(…)` discards errors), ≈L760–770 (record built with
  `Appearance::default()`), `load_startup_scripts` (≈L185–200).
- `crates/gradiance-script/src/reflect_bridge.rs` ≈L108–135: `write_path`
  and `try_apply_scalar`.
- `crates/gradiance-physics/src/lib.rs` ≈L110–122: `apply_sim_settings`
  copies `SimSettings` to avian every frame.
- `crates/gradiance-interaction/src/tools/mod.rs` ≈L190–200: the tools'
  default appearance.
- `crates/gradiance-domain/src/lib.rs`: brief 2-4 (#222) just added
  `#[require]` defaults on `Body`, including `RigidBody::Dynamic` inserted
  before `insert_into` replaces it. Relevant to bug 2.

## Facts (orchestrator, 2026-11-12, main `64a4543`)

- Bug 1 repro files: a script with `(sim-set "gravity.y" 0.0)` then
  `(spawn-ground 0 -2.5 0) (spawn-box -2 0 1 1) (spawn-box 2 0 1 1)`, run as
  `gradiance --script it.scm`. The bodies fall. No warning at
  `RUST_LOG=warn`. `write_path` errors are dropped, so a bad path, a
  rejected leaf type, or a later overwrite of `SimSettings` all look the
  same. *Unverified* which one it is; candidates: the write fails
  (`reflect_path_mut` on a glam `Vec2` field, or `try_apply` type
  mismatch); or something restores `SimSettings` after the startup run (the
  command-stack baseline, `commit_settings_edits`, a scene/environment
  apply). `docs/scripting.md`'s own example uses an integer (`-500`).
- Bug 2 was seen only under lavapipe, where frames are slow (long real time
  between frames). *Unverified* whether it reproduces headless at a normal
  frame rate; check both. Suspects, unranked: the 2-4 required-component
  insert order (`RigidBody::Dynamic` then replaced); mass/inertia or a
  centre-of-mass offset on boxes; a per-frame force (plane friction,
  fields, mouse spring/twist with the cursor resting over the viewport);
  catch-up stepping at a low frame rate.
- An existing pattern for settle tests: `tests/it/history.rs::spawn_settled`.

## Approach

1. **Bug 1.** Write a failing integration test first (startup-script and
   console-run variants). Find the cause; fix it. Make `sim-set` failures
   visible: log a `warn!` and surface them in the console the way script
   errors are. Accept integer literals for float fields. Fix
   `docs/scripting.md` if its example is wrong.
2. **Bug 2.** A failing headless test first: drop an unrotated box on a flat
   ground, step to rest, assert `|rot| < 1e-3` and no drift. Also run it with
   large per-frame deltas (simulating ~5 fps) if the plain version passes.
   Find the cause. If you can't reproduce it headless, say so and give the
   screenshot evidence for and against; don't guess a fix.
3. **Bug 3.** Script spawns use the same default appearance function the
   tools use (move it to a crate both can reach without an upward edge, if
   needed). A test asserts a scripted and a tool-drawn body get non-grey,
   id-derived hues by the same rule.
4. Re-run the screenshot scene after the fixes; post before/after images in
   the PR conversation, not in git.

Invariants: 4 (`SimSettings` stays a settings-seam write, never a command);
5 (no derived state saved). The scripting rules in CLAUDE.md (verbs queue
through existing seams).

## Out of scope

New verbs, UI changes, the keyboard-focus limitation of the harness,
`docs/state.md`. If bug 2 turns out to be deep in avian, stop at a minimal
repro + finding and return the brief.

## Done when

- Three regression tests (one per bug), each failing before its fix.
- Gates green (workspace tests via CI if disk is tight).
- Screenshots posted in the PR conversation (or described there with paths
  if the harness can't run).
- Brief Outcome filled with each root cause.

## Outcome

*(worker fills in)*
