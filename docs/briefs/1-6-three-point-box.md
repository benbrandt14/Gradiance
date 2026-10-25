# Three-point box
Phase 1 · M18 · "Alternate constructors: 3-point box, tangent circle (3.1)"
— the 3-point box · status: ready

## Goal

A box can be drawn by three clicks: the first two set one edge (its length
**and rotation**), the third sets the height on either side of that edge.
The result is a rotated `Box` body committed in one intent. The existing
two-corner drag stays the default; the 3-point constructor is chosen by a
config setting, so it is reachable from the settings UI and from a script
(`config` verbs) with no new chrome.

## Read first

- `crates/gradiance-interaction/src/tools/box_tool.rs` — the two-corner
  `DraftTool` and its pure unit test (drives `BoxTool` through a hand-built
  `ToolContext`, no ECS).
- `crates/gradiance-interaction/src/tools/polygon_tool.rs` — the existing
  multi-click draft pattern (vertices accumulate across clicks; Escape
  cancels).
- `crates/gradiance-interaction/src/tools/context.rs` — `DraftTool`,
  `ToolContext` (`cancel`, `confirm`), `ToolCommit::SpawnBody`, and
  `tools::new_body_record(shape, pos, rot)`.
- `crates/gradiance-domain/src/settings.rs` — `ToolDefaults` (≈L113): the
  config home for tool-creation preferences (already shown in Grid & Snap →
  Tools via a reflect grid).

## Facts (orchestrator, 2026-10-25)

- `BoxTool(Option<Vec2>)` commits on release: `ShapeDef::Box {width,
  height}` at the midpoint with rotation `0.0` (`box_tool.rs` ≈L20–45).
  `new_body_record` already takes a rotation.
- `ToolDefaults` is `Resource + Reflect` and rendered by
  `reflect_grid(…tool_defaults…)` in `ui/src/settings.rs` (Tools section);
  it is **not** persisted (workstation preference, like `DebugSettings`).
  Its only field today is `slider_limits: bool`. `ui/src/reflect_grid.rs`
  has **no enum arm**, so use a bool (e.g. `box_three_point`, default
  false) — it then appears in the Tools section with no UI code.
- Every tool point reads `ctx.cursor`, the snapped cursor, so the three
  clicks snap to grid/objects for free.

## Approach

1. Pure geometry in `gradiance-geometry` (e.g. alongside `snapping` or a new
   `construct` module): `three_point_box(a, b, c) -> Option<(center, width,
   height, rotation)>` — width = |b − a|, rotation = angle of b − a, height =
   signed distance of c from line ab (box extends to c's side). `None` for a
   degenerate edge or zero height. Unit-tested.
2. Extend the box tool (or add a sibling draft state) for the 3-point mode:
   click a, click b, move to preview the box, click c to commit. Escape
   cancels. In 2-corner mode nothing changes.
3. Mode lives in `ToolDefaults`. Respect `GestureConstraints` only where it
   is natural (the a→b edge may honour the axis lock); say what you did.
4. Preview: the edge after the second click, the full rotated rectangle
   while placing c (the `ToolPreview` primitives; add a rotated-rect helper
   only if needed).

Invariants: 1–2 (one `SpawnBody` commit at the third click, nothing
authored before it). Reflect: `ToolDefaults` already reflects.

## Out of scope

Tangent circle and the tangent snap kind (brief 1-7), alignment (1-5),
toolbar/glyph changes, any other constructor, `docs/state.md`.

## Done when

- Geometry unit tests for `three_point_box` (axis-aligned, rotated, c on
  either side, degenerate inputs → `None`).
- A pure tool test in `box_tool.rs` driving the three clicks through a
  hand-built `ToolContext`, asserting the committed shape, position and
  rotation; Escape mid-draft commits nothing.
- fmt, clippy `-D warnings`, doc `-D warnings`, per-crate tests green;
  workspace tests via CI if disk is tight (say so).
- `docs/roadmap.md` M18 constructors bullet: 3-point box landed.
- This brief committed with `## Outcome` filled.

## Outcome

*(worker fills in)*
