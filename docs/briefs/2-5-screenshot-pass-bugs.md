# Bugs found by the first screenshot pass
Phase 2 · substrate correctness · from the 2026-11-12 lavapipe screenshot
pass (`tools/screenshot/README.md`) · status: done (#223)

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

**Done**, PR #223. Each fix has a regression test that failed before the fix.
Gates: fmt, clippy, rustdoc, the `it` suite (3x threaded, 1x `--test-threads=1`),
workspace doctests, and the other crates' tests, all local; the full workspace run is in CI.

1. **`sim-set` — a timing bug, not a failed write.** The reflect write
   landed. But `apply_sim_settings` (`SimSettings` → avian `Gravity`) ran in
   `Update`, unordered against `run_scripts`, and physics runs at the start
   of the next frame. So one step after the script still used the old
   gravity. That step left freshly spawned bodies with a constant 1/6 m/s
   downward velocity (more under lavapipe catch-up), which zero gravity never
   removed. Fix: `apply_sim_settings` runs in `PostUpdate`, after every
   `Update` writer. `sim-set` now trials the write on the mirror, so a bad
   path or a non-number is a script error (warn + failed console entry). The
   drain `warn!`s instead of dropping errors. Integer and float literals
   already coerced alike; a test pins that. The `docs/scripting.md` examples
   are now SI (`-5`, `-25`). Tests: `scripting::a_startup_script_zeroes_gravity`
   (new `harness::headless_app_with` inserts `StartupScripts`),
   `a_console_script_zeroes_gravity_while_playing`,
   `integer_and_float_literals_both_write_a_float_setting`, and the unit test
   `a_failed_sim_set_is_a_visible_script_error`.
2. **Rotating boxes — caused by bug 1; reproduced headless.** The scene was
   the zero-g scene. The drifting boxes touched the ground, picked up a
   clockwise spin and a rightward velocity from the contact, and with no
   gravity they turned forever, in lockstep. With bug 1 reverted,
   `physics_sync::boxes_over_the_ground_in_zero_gravity_never_start_turning`
   fails (`-0.008 rad` after 20 s); with the fix the boxes never move. Under
   gravity, `a_box_dropped_flat_comes_to_rest_flat` (60 fps and ~5 fps
   catch-up; fixed-step results are identical) shows a flat drop settles to
   |rot| < 1e-3 and stops. **Not fixed, avian-internal:** a flat 2 m drop
   bounces asymmetrically (sequential corner contacts) and lands 1–4 cm
   sideways. The test bounds it at < 0.1 m. A follow-up would be a brief to
   evaluate avian solver settings, if it matters.
3. **Grey scripted bodies.** The hue rule moved to
   `Appearance::for_id` (`gradiance-domain`). Tools and the script
   `body_record` both call it. Test:
   `tools::scripted_and_tool_drawn_bodies_are_coloured_by_the_same_rule`.

4. **CI flake found on the way, and fixed (pre-existing; likely #221's
   first-run failure too).** `ci` failed once on this PR:
   `nodes::duplicating_a_body_copies_its_attached_tracer_and_bindings`
   panicked inserting `SignalColorOverride` on a despawned entity.
   `signal::evaluate_signals` is unordered against the command dispatcher
   in `Update`. When the executor ran it first in an undo frame, it queued
   the override insert for the clone, the dispatcher despawned the clone,
   and the insert then hit a dead entity. Fix: `try_insert`/`try_remove`
   (the override is derived, so a dead body needs none). Pinned
   deterministically by
   `nodes::a_body_despawned_while_its_override_is_queued_does_not_panic`,
   which drives the system by hand: run, despawn, then apply. The same
   unordered-derived-insert pattern may exist in other sync systems; I did
   not audit it.

Screenshots (lavapipe, after only; the orchestrator holds the before set):
`/tmp/gradiance-shots/after-*.png`, described in the PR conversation and not
committed.
