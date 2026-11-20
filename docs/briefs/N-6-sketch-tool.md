# Sketch tool
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A freehand stroke becomes one body: the stroke simplified (RDP) into
segments, each a capsule, joined as one SDF union. `polygonize` makes the
collider and mesh, so the tool adds no geometry path. A closed stroke
fills as a polygon instead.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `ShapeDef` is an SDF tree with `Csg` nodes; `Capsule` is a leaf.
- RDP simplification is pure math: it goes in `gradiance-geometry` with
  tests.

## Out of scope

Pressure-sensitive width.

## Done when

- A stroke of 200 samples gives one body of fewer than 30 leaves; undo
  removes it in one step.
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-n6`.

**Landed:**

- `gradiance-geometry/src/sketch.rs` (new): `rdp` (open-polyline
  Ramer-Douglas-Peucker, iterative), `simplify` (grows the tolerance by
  half until a point cap is met), `sketch_shape(stroke, radius)` → a
  `Sketch` (shape, body pose, closed flag, radius used, simplified line).
  An open stroke becomes at most `MAX_SKETCH_SEGMENTS` = 24 capsules under
  `Placed` nodes, joined by `balanced_union` (depth ≤ 7 <
  `MAX_CSG_DEPTH`); a one-segment stroke is a plain `Capsule` leaf rotated
  along it. A closed stroke (ends near, a simple ring, round enough) fills
  as a centroid-relative CCW `Polygon` (≤ 64 vertices). `leaf_count`,
  `ring_is_simple`, `resolvable_radius`. No new SDF leaf; `FORMAT_VERSION`
  unchanged.
- Contour resolution (`polygonize`, still the single discretization
  point): `sdf::thinnest_solid` (narrowest capsule on the solid side) and
  `polygonize::field_cell` give a tree with a capsule about 3 cells across
  it, within `SAMPLE_BUDGET` = 96² samples (what a square tree already
  takes) and never coarser than before; trees without a capsule contour
  exactly as before. `sdf::UnionField` evaluates a union of placed exact
  leaves bit-identically to `sdf::eval`, faster (precomputed rotations,
  box culling): a 24-leaf sketch contours in about 3 ms at dev opt-level 1
  (a box minus a circle: 0.7 ms). `contour::MAX_SAMPLES` is public.
- Operation `body.sketch` / `(sketch path width)`
  (`command/src/ops/sketch.rs`): one `SpawnBodyIntent`, undo label
  `spawn-body`, returns the handle. `docs/ops.md` regenerated.
- Interaction: `ToolState::Sketch`, `sketch_tool::SketchDraft` (records the
  unsnapped pointer every 3 px; at `MAX_STROKE_POINTS` = 512 it keeps every
  other point and samples half as often, so memory is bounded; preview:
  the faint stroke, the simplified centre line with a ring of the stroke
  width at each joint, or the closed outline), `ToolCommit::Sketch` → an
  `InvokeOp` of `body.sketch` with the stroke thinned to 1 px, so one
  gesture is one step and Adjust Last Operation changes a drawn stroke's
  width. `tool.sketch`, **no key**, status-bar hint. No editor key binding
  changed.
- Settings: `ToolDefaults::sketch_width` (10 cm; workstation config) shows
  as a unit field in Settings > Grid & Snap > Tools.
- UI: palette entry "Sketch" in Create after Polygon (`sketch.png`, no
  key). Tool palette snapshot regenerated (canvas 374 → 396 px; only the
  Sketch row is new). The settings-grid snapshot does not show the Tools
  section and is unchanged.
- Tests: `tests/it/sketch.rs` —
  `a_stroke_of_200_samples_is_one_body_of_under_30_leaves_and_one_undo_step`
  (the "done when": a 200-sample tool drag; one Csg body, 2..30 leaves,
  depth ≤ 8, covers the stroke, one collider, one contour piece, one
  `spawn-body` step through `body.sketch`; undo removes it, redo restores
  it), `a_closed_stroke_fills_as_a_polygon`,
  `a_straight_stroke_is_one_capsule`,
  `the_width_comes_from_the_tool_defaults_and_adjusts_after`,
  `a_click_makes_no_body`, `the_operation_refuses_a_bad_path_or_width`,
  `a_sketched_body_falls_and_rests_on_the_ground` (a J lands on a ground
  plane and settles in 4 s without sinking). Unit tests in geometry
  (RDP bounds, caps, closing rules, figure eight, widening, union depth,
  `thinnest_solid`, `field_cell`, `UnionField` bit-exactness, two thin
  rods contour as one piece) and interaction (sampling cap, invocation).
- Docs: `docs/scripting.md` (`sketch`), `docs/sdf-geometry-decision.md`
  (resolution follows thin leaves).

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`sketch-shape`, `sketch-closed`, `sketch-resolution`, `sketch-sampling`,
`sketch-defaults`, `sketch-tool-key`.

**Learned:** body borders re-contour every body every frame
(`render::decorations::draw_body_borders`), so the cost of a CSG body's
contour is a per-frame cost, not a spawn cost. That is why the sketch's
finer cells stay within the existing sample count and why the union is
evaluated through `UnionField`. A per-body derived `Contours` cache (the
SDF decision's original plan) would remove the per-frame cost for every
CSG body; it is outside this brief.

**Not done / follow-ups:** not checked on screen (no screenshot run): the
stroke preview and the icon in the running app. A very large scribble (a
12 m drawing) widens a thin stroke to about 25 cm so the contour keeps it
whole (decision `sketch-resolution`). Pressure-sensitive width stays out
of scope. A closed stroke fills to its centre line, not its outer edge.
