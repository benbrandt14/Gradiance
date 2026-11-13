# No raw UUIDs, no stray rows
Phase 2 · polish of landed UI · from the 2026-11-13 screenshot pass
(`tools/screenshot/`) · status: done

## Goal

Two defects visible in the running editor are fixed:

1. **Raw UUIDs in user-facing labels.** The joint inspector's title reads
   "b9957241-9891-417b-a478-c899afda6288 to world pin". The context menu's
   "Select from" list reads "· body 0 (570fd0ef-dc78-4c43-b6f4-70746c7f8fbb)".
   The outliner already shows readable labels ("box · 0e8593"). Every
   user-facing body/joint reference should use that same label rule.
2. **A stray "system (unsupported)" row** in Settings → Grid & Snap, right
   below the real "system" picker.

## Read first

- `crates/gradiance-ui/src/outliner.rs` ≈L83 `short_id` and ≈L110–130 (the
  "kind · short id" labels), the rule to reuse.
- `crates/gradiance-ui/src/joint_inspector.rs` ≈L60–66 and
  `crates/gradiance-ui/src/context_menu.rs` ≈L585–595: the two
  `format!("{:.8}", id)` sites.
- `crates/gradiance-core/src/ids.rs`: `StableId`'s `Display`.
- `crates/gradiance-ui/src/reflect_grid.rs` ≈L80–95 (the "(unsupported)"
  fallback) and `crates/gradiance-ui/src/settings.rs` ≈L145–180
  (`grid_snap_tab`: the explicit system picker, then `reflect_grid_units`
  over the whole `GridSettings`).
- `docs/ui-design.md`: labels and weak text.

## Facts (orchestrator, 2026-11-13, main `f9c4702`)

- **Cause of 1:** both sites write `{:.8}` expecting an 8-char prefix, but
  `StableId`'s `Display` ignores the precision flag, so the full UUID
  prints. *Unverified:* whether other `{:.N}` uses of `StableId` exist (grep
  the workspace, including log lines; logs may keep full ids).
- Workspace labels (`(label body "name")`, `WorkspaceLabels`) exist. The
  outliner may or may not prefer them; match whatever the outliner does, so
  every surface agrees.
- **Cause of 2:** the tab draws its own picker for the `system` enum, then
  hands the whole struct to `reflect_grid_units`, which renders enum fields
  it can't edit as "(unsupported)". The fix is to skip fields the caller
  already rendered (an exclude list), not to teach the reflect grid enums.

## Approach

1. One shared label function (domain or ui level, wherever the outliner's
   rule can live without an upward edge) used by the outliner, the joint
   inspector and the context menu. Either make `StableId`'s `Display`
   honour precision, or stop using `{:.N}` at those sites; say which.
2. `reflect_grid_units` (or its caller) skips fields the tab already drew;
   `system` disappears from the generic rows.
3. Screenshots after the fix (context menu "Select from", joint inspector
   title, Grid & Snap tab) via `tools/screenshot/`. Describe them in the PR,
   keep them out of git, report their absolute paths. Tip: `(panel-show
   "properties")` and `(panel-show "settings")` in a startup script open
   panels without menu clicks, which are unreliable at lavapipe frame
   rates.

## Out of scope

Where the joint inspector lives (floating window vs the Properties tab),
degree/radian display (Phase 5 units P4), other UI changes, new labels for
nodes, `docs/state.md`.

## Done when

- A unit test for the shared label (no UUID longer than the short form).
- A test (unit or integration) that the Grid & Snap rows don't include
  `system` twice, if the reflect grid is testable headless; otherwise say
  so and rely on the screenshot.
- Gates green; screenshots described in the PR. Outcome filled.

## Outcome

**done** (PR #226).

- **Label rule:** `gradiance-ui/src/labels.rs` now owns `short_id` (6-char
  prefix), `object_label(kind, id)` → `"box · 0e8593"`, and
  `body_label(Option<&ShapeDef>, id)` (falls back to "body"). The outliner,
  the joint inspector title ("box · b0c9d8 to world pin") and the context
  menu's "Select from" all use it. Workspace-labelled bodies put their
  script name in the kind slot ("wheel · fc45ee"); the outliner does not
  read workspace labels, unchanged. The "body {i}" index in "Select from"
  is gone (it was the under-cursor order, not a script index).
- **Display vs precision:** chose to *stop using `{:.N}`* on `StableId`;
  its `Display` is unchanged (forwards to the UUID). Other sites found and
  moved to `short_id`: `ports.rs` plot-binding name (`{label}@{id:.4}`) and
  three in the Debug tab's readouts (`settings.rs`: selection ids, authored
  joint list). No log line used `{:.N}` on an id. `signal.rs`'s
  `"{tag}@{id}"` is a machine key and keeps the full id.
- **Stray row:** `reflect_grid_except(…, skip)` (`reflect_grid_units` is it
  with no skip); Grid & Snap passes `settings::GRID_HAND_DRAWN = ["system"]`.
- **Tests:** `labels::tests` (label is kind + short id, never the full
  UUID; short id stable), `reflect_grid::tests` (the row list for
  `GridSettings` drops `system` and keeps `spacing`; tested on the pure
  `grid_rows`, no egui context needed).
- **Gates:** fmt, workspace clippy, `cargo doc -D warnings`,
  `cargo test -p gradiance-ui` green locally (rustc 1.97.0 = `RUST_PIN`).
  `cargo test --workspace` left to CI (under 20 GB free).
- **Screenshots** (not committed): context menu on a box and on a labelled
  circle, the joint inspector, the Grid & Snap tab; paths in the PR.
- Harness note: on this pass `xdotool click` worked for egui widgets and
  mousedown/up did not; scene picks and right-click needed mousedown,
  one second, mouseup. Spawn verbs take metres.
