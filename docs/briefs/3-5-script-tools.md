# Tools authored in a script
Phase 3 · scripting P3 · "User tools from `.scm`: a tool is a `ToolContext
→ (preview, commit-intent)` (M17's `DraftTool`) / `ManipTool` closure; the
registry lets one be authored in lisp and registered by name, reusing the
exact press/drag/release driver the built-in tools use." · status: ready

## Goal

A `.scm` file can add a **tool** that appears in the tool palette, collects
points from the user like the built-in creation tools, previews the gesture,
and on completion runs the script's body once, with the collected points
readable. Example: a "ramp" tool takes two clicks and spawns a rotated box
between them. One gesture = one script run = one undo batch.

## The constraint that shapes the design

CLAUDE.md's two-tier perf rule: **the scripting VM must never run in the
per-frame loop.** A tool's `update` and `preview` run every frame. So the
roadmap's "a closure implementing `DraftTool`" cannot literally be a
per-frame call into steel.

The intended shape:
- a generic, Rust-side **`ScriptTool`** draft implements `DraftTool`;
- it collects points with the same driver, snapping and `SnapReference`
  publishing as the built-ins;
- it draws a generic preview (the points and the polyline through them);
- when the gesture completes, it submits the tool's body **once** through
  `ScriptInputs`, with the points as that run's context. That is the
  per-run context mechanism 3-3 added for the click point
  (`ScriptRun { at }` → `(action-x)`).

If you find a better shape that still keeps the VM off the frame path, take
it and explain it in the Outcome. Any shape that calls the VM per frame is
out.

## Read first

- `crates/gradiance-interaction/src/tools/context.rs` ≈L300–340
  (`DraftTool`, `run_draft_tool`, `SnapReference` publishing) and
  `polygon_tool.rs` (a multi-click draft; confirm/cancel).
- `crates/gradiance-script/src/actions.rs` and `bridge.rs`
  (`register-action`, `ScriptInputs::invoke`, `ScriptRun { at }`, the
  `(action-x)`/`(action-y)` verbs): the per-run context to extend.
- `crates/gradiance-core/src/states.rs`: `ToolState` is a `Copy` enum,
  one variant per tool, gating each tool's systems.
- `crates/gradiance-ui/src/toolbar.rs`: the palette (PNG icons, an
  existing-icon convention from 2-3).
- `docs/scripting.md` and the CLAUDE.md scripting rules (a verb queues
  through an existing seam; reads never reach up; one snapshot per run).

## Facts (orchestrator, 2026-11-13, main `df6d425`)

- `register-action` stores `{label, source}` in a table the UI renders;
  invoking submits a `ScriptRun`. 3-3 added per-run context (`at`) that a
  run reads and only that run sees. 3-1: a run is all-or-nothing with a 2 s
  budget.
- Tool switching is `ToolState` plus `in_state` gating. A registered tool
  needs a state that names it. *Unverified:* whether a single
  `ToolState::Script` variant plus a resource holding "which script tool"
  works with the palette's highlight logic and the input map; or whether
  `ToolState` can carry a small index and stay `Copy` + `Hash`. Pick one;
  say why.
- Tools from a `.scm` are not persisted (like actions and labels); they
  live while the script is loaded. Hot reload re-registers by name
  (upsert, as actions do).

## Approach

1. `(register-tool name points src)`: `points` = clicks to collect (≥ 1);
   `src` runs on completion. Reads in that run: `(tool-count)`,
   `(tool-x i)`, `(tool-y i)` in `spawn-*` units. NaN or an error outside a
   tool run, matching 3-3's convention.
2. `ScriptTool` draft: press = add point; Escape/right-click = cancel (as
   polygon does); last point = submit. Preview: points + polyline. Publish
   `SnapReference::Line(last point)` so tangent snap works mid-gesture.
3. Palette: registered tools appear after the built-ins with a generic
   existing icon and their name as tooltip/label. Thinnest UI that works.
4. Registry: an `OpSpec` row for each new verb.

Invariants: 1–2 (the run emits intents through existing verbs; nothing
mutates during the gesture except preview gizmos); the two-tier rule (VM
once per gesture); `steel` stays in `gradiance-script`; no upward
dependency edges. If interaction must learn about script tools, it does so
through a resource the script layer publishes, not an edge to
`gradiance-script` (check `tests/boundaries.rs`).

## Out of scope

Script-side per-frame previews, drag-based (non-click) script tools,
`ManipTool` (selection/move) script tools, custom icons, persisting tools in
the scene, keyboard shortcuts for script tools, `docs/state.md`.

## Done when

- Integration test: register a two-point "ramp" tool; drive two presses
  through the tool driver headlessly; one box spawns, rotated along the
  segment between the points; one undo removes it. Cancel mid-gesture spawns
  nothing. A test that the VM is not entered while points are being
  collected (e.g. a counter on script runs stays 0 until the last point).
- Screenshot via `tools/screenshot/` of the registered tool in the palette
  and a ramp it drew. Describe it in the PR, keep it out of git, report the
  path. Register the tool in a startup script; select it by clicking its
  palette slot.
- `docs/scripting.md` § Script tools; roadmap P3 "User tools from `.scm`"
  marked landed. Gates green. Outcome filled.

## Outcome

**Status: done** (PR and CI in the PR thread; screenshot described there).

**Shape: the brief's, unchanged.** One Rust `ScriptTool` draft
(`crates/gradiance-interaction/src/tools/script_tool.rs`) implements
`DraftTool` and runs under the same `run_draft_tool` / `draw_draft_preview`
as box, circle and polygon: the same snapping, `SnapReference::Line(last
point)` for tangent snap, a preview of the points (gold markers) and the
polyline plus a rubber band to the cursor. Press adds a point; Escape or a
right-click cancels; the last point commits `ToolCommit::Script { tool,
points }`, which the driver writes as a `ScriptToolFired` message. The VM is
entered once per gesture, in `run_scripts`; nothing during collection calls
it.

**Where the layers meet: `gradiance-core::script_tools`.** Interaction and
script may not depend on each other, so the table and the message live in
core (no new crate edge): `ScriptTools` (`{name, points, source}`, upsert by
name, never shrinks) is written by the script layer (`register-tool` →
`drain_tools`) and read by interaction (point count) and UI (palette);
`ScriptToolFired { tool, points }` is written by the tool driver and drained
by `run_scripts` (`queue_tool_runs` → `ScriptInputs::invoke_tool`). The
per-run context extends 3-3's mechanism: `ScriptRun` gained `points`, set on
the engine around that run only and read by `(tool-count)`, `(tool-x i)`,
`(tool-y i)`.

**ToolState: `ToolState::Script(u16)`** (an index into `ScriptTools`), not a
single `Script` variant plus a "which tool" resource. It stays `Copy + Eq +
Hash + Reflect`, and:
- the palette's highlight is the existing `current == state` comparison, with
  no second resource to consult;
- switching from one script tool to another is a real state change, so the
  existing `state_changed::<ToolState>` hook clears the snap reference, and
  `sync_script_tool` (run before the driver) drops the draft when the index
  or the point count changes;
- gating is one run condition (`script_tool_active`, `matches!` on the
  variant) instead of `in_state`.
The index is stable because entries are upserted in place and never removed
(no unregister verb); an index the table lacks is an inactive tool.

**Verbs (one `OpSpec` row each):** `register-tool name points src`
(EditorState; points a whole number 1..=64, else a script error; rolled back
with a failed run like actions), `tool-count`, `tool-x i`, `tool-y i`
(Query; NaN outside a tool run as `(action-x)`; an index past the last point
inside a run is an error, since it can only be a bug in the body).

**One addition the Done-when needed: `spawn-box-rot x y w h angle`.** The
brief's ramp is a *rotated* box, and no existing verb can make one in a
single run: `spawn-box` has no angle, and `place` indexes the pre-run
snapshot, so a run cannot rotate a box it just spawned (scripting rule 3).
It is `spawn-box` plus `pose.rot`, the same `SpawnBodyIntent`, with an
`edit_bindings` row. An optional fifth arg on `spawn-box` was rejected:
`OpSpec.args` is a fixed arity (the remote catalog reports it) and steel's
rest args are private. steel has no two-argument `atan`; the ramp uses
`(angle (make-rectangular dx dy))`.

**Palette:** after the built-ins, a separator, then one button per script
tool with the existing `icons/sketch.png`, the name as a small caption and
hover text ("ramp (script tool, 2 clicks)"); text label headless.
`ToolContext` gained `secondary` (the right button's phase), read only by the
script tool, so the built-in tools' cancel rules are unchanged.

**Tests.**
- `tests/it/script_tools.rs`:
  `a_two_point_ramp_spawns_one_rotated_box_and_one_undo_removes_it` (drives
  two clicks through the real driver; the `ScriptLog` run count stays at the
  registration's 1 after the first point and becomes 2 after the last; box
  width = segment length, centre = midpoint, rotation = segment angle; one
  undo removes it), `cancelling_mid_gesture_spawns_nothing` (right-click and
  Escape both drop the draft; no run), `tool_reads_are_nan_outside_a_tool_run`,
  `a_bad_point_count_is_a_script_error`.
- `tests/it/ui_panels.rs`: `toolbar_palette_lists_script_tools_after_the_builtins`.
- Unit: `script_tool::tests` (commit on the last point with every point;
  right-click cancel; no active tool collects nothing),
  `core::script_tools::tests` (upsert keeps the index), bridge tests
  (`register_tool_publishes_the_tool_table_and_upserts_by_name`,
  `a_fired_tool_runs_its_body_once_with_its_points`,
  `a_tool_point_past_the_last_is_an_error`).

**Gates (local):** fmt, clippy `--workspace --all-targets -D warnings`, doc
`-D warnings`, `cargo test -p gradiance-core -p gradiance-script
-p gradiance-interaction`, `cargo test --test it` (script_tools, scripting,
ui_panels, registry_validation, remote, tools). The workspace test run was
left to CI (about 12 GB free on the container, under the 20 GB bar).

**Screenshot** (PR #230 comment): the palette shows a "ramp" button
(sketch icon, caption) after a separator below Tracer. The first click
snapped to the ground surface and drew a gold point marker and a rubber band,
with nothing spawned. The second click produced one thin box lying exactly on
the segment.

**Learned:** in the headless harness, `ButtonInput<KeyCode>::press` loses
`just_pressed` to the input plugin's per-frame clear before `Update`. So
`keys.just_pressed(Escape)` never fires that way, which also means no existing
test exercised the polygon's Escape cancel. Send a `KeyboardInput` message
instead (`tests/it/script_tools.rs::escape`). Mouse presses work because the
tools read `PointerButtons`.

**Follow-ups (not done; out of scope):**
- A run with several edits is several undo steps (one per command), as for
  any script run; "one gesture = one undo batch" holds for single-edit tools
  like the ramp. A grouped undo step per run would be a command-layer change.
- Drag-based script tools, `ManipTool` (selection-reading) script tools,
  script-side previews, custom icons, keyboard shortcuts (all out of scope
  per the brief).
- No `unregister-tool`; a renamed tool in a hot-reloaded file leaves the old
  entry in the palette until restart.
- The palette's script section grows one row per tool; many tools would want
  a scroll area or a grid.
