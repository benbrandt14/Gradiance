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

*(worker fills in)*
