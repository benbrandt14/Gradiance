# Curvature tool and Pen tool
Phase E · roadmap § "Unified plan" · status: ready (after E-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 8). Docs only.

## Goal

Two ways to draw a path body:

- **Curvature tool** (the default for smooth shapes): click points; the
  path through them is smooth, each run of three following the circle
  through them; `Alt`-click (or a double-click) makes a corner; clicking
  the first point closes it; `Enter` ends an open path (it becomes a rod
  of the tool's width). The preview curve follows the pointer.
- **Pen tool**: click for a corner, click-drag for a smooth anchor with
  symmetric handles, a modifier breaks the symmetry.

Both snap through C-1, take typed segment lengths through C-6, and commit
one `spawn-body` step.

## Read first

- `docs/interaction-feel.md` — row **Illustrator**; C-1 and C-6 for the CAD parts.
- `crates/gradiance-interaction/src/tools/polygon_tool.rs` — the click-by-click draft precedent (close-on-first-vertex radius).
- VectorCraft `crates/tools/src/draw2.rs` (`curvature_spline`, `smooth_handles`, `end_handle`) and `crates/tools/src/pen.rs` at `9fff48d`.

## Facts

- VectorCraft's `curvature_spline` is pure and ~60 lines plus helpers; it
  ports into `geometry::path` almost verbatim (point type aside).
- Modifier clash check: `Ctrl` is duplicate-drag in select; `Shift` is
  selection-only; the polygon tool's draft must not lose a click to
  click-select (M16 fall-through rule).

## Approach

- Feel: **Illustrator**. `geometry::path::curvature_spline(points, closed)`
  (vendored from VectorCraft with its tests); the two tools are `DraftTool`s;
  open-path width is `ToolDefaults::path_width` (Settings > Grid & Snap >
  Tools), shared with the sketch stroke width.
- Palette entries in Create (Curvature, Pen), no default keys (keys are
  unsettled); `tool.curvature`, `tool.pen` operations.
- Adjust Last on the width and on closed/open.

## Out of scope

Editing after commit (E-3).

## Done when

- Pure tests (ported): three points on a circle give handles tangent to
  it; a corner flag gives no handles.
- Behaviour tests: four clicks + close make a closed path body whose area is
  within 2 % of the circle through them; Enter on an open draft makes a rod.
- Gates green; screenshots of both tools on the PR.

## Outcome
