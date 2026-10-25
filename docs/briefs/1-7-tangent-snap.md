# Tangent snap and tangent circle
Phase 1 · M18 · "Alternate constructors: … tangent circle (3.1)" and "Snap
glyph … tangent glyph (3.4)" · status: done

## Goal

Two linked features. (1) A **tangent snap**: while a draft has a reference
point (the previous click of the polygon, strut, or 3-point-box tool), the
cursor snaps to the points on nearby circles where a line from that
reference would be tangent, shown with its own glyph. (2) A **tangent
circle**: with the circle tool, dragging the radius near another body's
outline snaps the radius so the new circle just touches it. Feedback 3.4:
"tangent should at least have a glyph here"; 3.1 names the tangent circle.

## Read first

- `crates/gradiance-interaction/src/snap.rs` — `SnapKind`, `SnapExclusions`
  (the precedent for a tool-set resource the snapper reads), the circle arm
  that `snap_outline` feeds.
- `crates/gradiance-geometry/src/snap_features.rs` — `SnapOutline::Circle`,
  `nearest_on_circle`.
- `docs/briefs/1-4-snap-glyph-stability.md` § Outcome, "For 1-6 (tangent
  snap)" — the formula and the reference-point gap, already worked out.
- `crates/gradiance-interaction/src/indicators.rs` — `draw_snap_indicator`
  (exhaustive match over `SnapKind`).
- `crates/gradiance-interaction/src/tools/circle_tool.rs` — the radius drag.

## Facts (orchestrator, 2026-10-25)

- Tangent point from `q` outside a circle (c, r):
  `c + r·rot(±acos(r/|q−c|))·(q−c)/|q−c|` — compute in the shape's frame
  (inverse affine, as the edge projection in `snap.rs` does), map back with
  `world_point` (1-4 Outcome).
- There is no reference point in the snapper today. The 1-4 Outcome
  suggests a small resource the tool sets at its first click and clears on
  commit/cancel, mirroring `SnapExclusions`.
- `CircleTool(Option<Vec2>)` stores the centre; radius = `center.distance(p)`
  (`circle_tool.rs` ≈L25–38). A tangent radius against any outline is the
  distance from the centre to the nearest outline point (for a circle:
  `|c₂ − c| − r₂` outside, `r₂ − |c₂ − c|` inside).
- Brief 1-6 landed (#217): the box tool's 3-point mode stores its placed
  points in `BoxDraft::EdgeStart(a)` / `Height(a, b)`. `DraftTool` has only
  `update`, `drafting`, `preview` — there is no way to publish a reference
  yet. The 1-6 worker's suggestion: a default trait method
  `fn snap_reference(&self) -> Option<Vec2> { None }` that `run_draft_tool`
  copies into the reference resource each frame (cleared when none).
  Implement it for the polygon (last vertex), box 3-point (last placed
  point) and strut (first anchor, if it is a draft tool — check).

## Approach

1. Pure math in geometry: `tangent_points(q, center, radius) -> Option<[Vec2;
   2]>` (`None` when `q` is inside/on the circle) and a
   `tangent_radius(center, outline)` helper. Unit-tested.
2. `SnapKind::Tangent`; a reference-point resource (e.g. `SnapReference(Option<Vec2>)`)
   set by multi-click tools at each placed point and cleared when the draft
   ends. The snapper offers tangent candidates for circle outlines near the
   cursor when a reference exists, competing in the existing
   nearest-candidate rule. Give it a distinct glyph.
3. Circle tool: while dragging the radius, if the cursor is within the
   capture radius of the tangent radius to a nearby outline, snap the radius
   to it and show the tangent glyph at the touch point. Commit is unchanged
   (one `SpawnBody`).
4. CSG shapes keep their contour path (circles inside CSG are out of scope —
   same follow-up as 1-4's).

Invariants: none authored changes; snapping is per-frame derived state; one
commit per gesture (2).

## Out of scope

Tangent-tangent-radius circles, tangents to non-circular curves, CSG-arc
features, alignment (1-5), `docs/state.md`.

## Done when

- Geometry tests: tangent points are on the circle and perpendicular to the
  radius (`(t−c)·(t−q) ≈ 0`); `None` from inside; tangent radius to circle
  and to box outlines.
- Integration test: a polygon draft with a first vertex placed, cursor near
  a circle's tangent point → `SnappedCursor.kind == Some(Tangent)` at the
  tangent point. Integration or pure-tool test (the tool tests in `box_tool.rs` show the hand-built-`ToolContext` pattern; `ToolContext` now also carries `defaults: &ToolDefaults`): a circle drawn near another
  commits a radius that touches it (distance between centres = r₁ + r₂).
- fmt, clippy `-D warnings`, doc `-D warnings`, per-crate tests green;
  workspace tests via CI if disk is tight.
- `docs/roadmap.md` M18: tangent circle and tangent glyph landed.
- This brief committed with `## Outcome` filled.

## Outcome

**Landed.**

- **Math** (`geometry::snap_features`): `tangent_points(q, c, r) ->
  Option<[Vec2; 2]>` (`None` from inside/on the circle or r ≤ 0) and
  `tangent_radius(center, &SnapOutline) -> Option<TangentTouch {point,
  radius}>` — the nearest outline point (circle: radial; contours: nearest
  segment projection), so it is external from outside, internal from
  inside. `None` at a circle's own centre or on the outline. Five unit
  tests (on-circle + perpendicular, a known 60° case, none from inside,
  circle outside/inside, box side/corner/inside).
- **Reference seam** (`interaction::snap`): `SnapReference(Option<DraftReference>)`
  with `DraftReference::Line(p)` (a segment drawn from `p`) and
  `::Radius(c)` (a circle drawn about `c`). Both `DraftTool` and
  `ManipTool` got a default `fn snap_reference(&self) -> Option<DraftReference>
  { None }`; `run_draft_tool` / `run_manip_tool` copy it into the resource
  each frame (write on change only), and a `clear_snap_reference` system
  runs on `state_changed::<ToolState>` so an abandoned draft leaves none.
  The snapper reads it one frame late (it runs in `PreUpdate`), which is
  invisible in use.
- **Publishers:** polygon (last vertex), box three-point (`EdgeStart(a)` →
  a, `Height(a, b)` → b; the two-corner drag publishes nothing — an
  axis-aligned box has no free segment), strut (first anchor; it is a
  `ManipTool`, hence the second trait method), circle (`Radius(centre)`).
- **Circle tool:** no change to its update or commit. In `Radius` mode the
  snapper queries every body within `|p − c| + capture` of the centre and
  scores each touch point by `||p − c| − r_touch|`, so the radius snaps at
  any angle around the circle; the reported cursor *is* the touch point, so
  the committed radius is exact and the glyph sits where the circles meet.
- **Glyph:** `SnapKind::Tangent` draws CAD's tangent marker, a circle
  (0.7 s) with a horizontal line resting on top.
- **Integration tests** (`tests/it/interaction.rs`): polygon draft from
  (150, 0) to an r = 50 circle → `Tangent` at the exact tangent point
  (and `Edge` at the same cursor before the first click and after leaving
  the tool); circle tool beside a circle at (200, 0) → committed r = 150 =
  d − r₂, both facing the neighbour and from 90° away, and a free radius
  outside the capture.

**Judgment call — tangent priority, not the plain nearest rule.** The brief
said tangents compete in the existing nearest-candidate rule. They cannot
usefully: a circle's own `Edge` projection is the nearest point on the
circle, so it is never farther than a tangent point, and the tangent would
win only inside `EDGE_TIE_BIAS` (0.6 px at default capture). Tangents are
therefore a tier above the other object snaps inside the capture radius
(`Nearest::winner` in `snap.rs`). They are offered only while a draft has a
reference, so this never changes plain snapping — every 1-4/1-5 test is
unchanged.

**Not done / for later.**

- No `SnapSources.tangents` toggle: adding one changes the saved
  `SnapConfig` and the Grid & Snap panel. Tangents are on whenever object
  snaps are.
- CSG shapes with circular leaves offer no line tangents (Contours path),
  same follow-up as 1-4's. The tangent *radius* does work against any
  outline, CSG included, because it uses the contour.
- The radius-mode query scans the whole disc of the drag, polygonizing each
  body in it each frame. Fine at sandbox scale; cache outlines if a big
  scene shows it.
- Gates: fmt, clippy `-D warnings`, doc `-D warnings`,
  `cargo test -p gradiance-geometry -p gradiance-interaction`, and the whole
  `tests/it` binary (245/245) run locally. `cargo test --workspace` was left
  to CI (container had 11–17 GB free, under the 20 GB bar). rustc 1.97.0 =
  `RUST_PIN`.
