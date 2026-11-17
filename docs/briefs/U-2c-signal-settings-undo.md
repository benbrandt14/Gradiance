# Signal-graph edits become undo steps
Phase U · CLAUDE.md invariant 4 · found by U-2b (Outcome, decision 6) ·
status: open

## Goal

An edit to the signal-graph resources (`SignalBindings`, `SignalParams`,
`ComputedSignals`) records one settled `settings` undo step, as an edit to
`SimSettings` does. `defparam` and `defsignal` then declare
`undo: Some("settings")`, and conformance test 6 checks it.

## Read first

- `CLAUDE.md` invariant 4 (scene content vs workstation config).
- `crates/gradiance-command/src/lib.rs`: `commit_settings_edits` (line 347)
  and `SceneSettings` (its four fields).
- `crates/gradiance-scene/src/records.rs`: `EnvironmentRecord::
  scene_content_eq` and `apply_scene_content` (both include the three
  signal resources).
- `docs/briefs/U-2b-queries-config-editor-ops.md` § Outcome, decision 6.
- `tests/it/op_conformance.rs`, the history test.

## Facts (planner, 2026-11-17, measured on `744e452`)

- `scene_content_eq` and `apply_scene_content` include `signals`,
  `params` and `computed`. `SceneSettings` holds only `sim`, `render`,
  `lighting` and `scenery`. Thus undo restores the signal resources when it
  restores some other settings step, but an edit to them alone records no
  step.
- U-2b measured no step for `(defparam …)` and `(defsignal …)`.
- Not checked: whether the signal panel records its edits by another path.
  Check this first. If it does, the panel and the script must use one path.

## Approach

1. Add the three signal resources to `SceneSettings` (or derive
   `SceneSettings` from the `EnvironmentRecord` scene-content fields, so
   the two lists cannot differ again; prefer this if it stays small).
2. Set `undo: Some("settings")` on `signal.param` and `signal.computed`.
   Decide for `signal.set` by the same rule (it writes a live value, so it
   is likely not scene content); record the decision.
3. Tests: an edit to each signal resource records one step, undo restores
   it, and a slider-style run of edits settles into one step.

## Out of scope

Any other settings resource, the signal panel's layout, new verbs.

## Done when

- The three resources produce settled undo steps; the tests above pass.
- Conformance test 6 checks `defparam` and `defsignal` with no special case.
- fmt, clippy, doc local; CI green. Outcome filled.

## Outcome

(open)
