# Steady snap glyphs on curves
Phase 1 · M18 · "Snap glyph stability, tangent glyph, snap-off-when-grid-
hidden (3.4)" — stability + snap-off · status: ready

## Goal

Hovering near a circle's outline shows one steady snap glyph instead of
flickering between kinds frame to frame. Feedback 3.4: "circle snap glyph
rapidly changes between type". Also pin, with a test, that grid snap is off
whenever the grid is hidden.

## Read first

- `crates/gradiance-interaction/src/snap.rs` — `update_snapped_cursor`, the
  object-snap loop and the grid-snap gate.
- `crates/gradiance-interaction/src/indicators.rs` — `draw_snap_indicator`.
- `crates/gradiance-geometry/src/polygonize.rs` ≈L20–35 — circles become
  `CIRCLE_SEGMENTS` vertices.
- `docs/sdf-geometry-decision.md` — skim for whether analytic leaves are
  meant to be queried directly (they are the source of truth; polygonize is
  the discretization point).

## Facts (orchestrator, 2026-10-24, on main `5bb18b7`)

- Object snap polygonizes every shape and offers each ring **vertex**,
  **midpoint**, and nearest-**edge** point as candidates (`snap.rs:107–129`).
  For a circle that is `CIRCLE_SEGMENTS` vertices + as many midpoints, all
  tessellation artifacts — no real feature lives there. Moving along the
  outline crosses vertex/midpoint/edge capture regions continuously: the
  likely cause of the flicker. *Unverified:* reproduce before fixing.
- Edge candidates get `+ radius * 0.05` so vertices/midpoints win ties
  (`snap.rs:126`).
- Snap-off-when-hidden **already exists**: the grid-snap block requires
  `grid.visible` (`snap.rs:142–145`). It is not pinned by a test that names
  that behaviour — *unverified*; check `tests/it/interaction.rs`.
- CLAUDE.md: `geometry::polygonize` is the single discretization point;
  "never contour a field anywhere else." Querying an analytic leaf's own
  features (a circle's centre and radius) is not contouring, but keep any
  new feature math in `gradiance-geometry`, not in `snap.rs`.

## Approach

1. Reproduce: a test that sweeps the cursor along a circle's outline and
   records `SnapKind` per step; today it should alternate.
2. Fix at the source: snap candidates on smooth outlines should be real
   features. Suggested: for analytic curved leaves (a bare `Circle`, possibly
   a `Placed` circle), offer centre, the four quadrant points as vertices,
   and the exact nearest point on the circle as the edge — no tessellation
   vertices/midpoints. Put "snap features of a `ShapeDef`" in
   `gradiance-geometry` as a pure function; CSG trees keep the polygonized
   path. If you find the better fix is hysteresis instead (stick with the
   last kind within a margin), you may — explain why in the Outcome.
3. Add the snap-off-when-hidden test if missing.

Invariants: none (snapping is derived, per-frame state). Glyph shapes stay
as they are.

## Out of scope

The **tangent** snap kind and glyph — it needs a reference point (the
previous click) and belongs with brief 1-6's tangent-circle constructor;
note anything you learn for it in the Outcome. Also out: grid line styling
(1-2), axis-lock basis (1-3), collinear/centreline snapping (1-5),
`docs/state.md` (orchestrator updates it).

## Done when

- The sweep test asserts one stable kind (or a documented small set with no
  alternation) along a circle's outline.
- Geometry-crate unit tests for the new feature function.
- A test that grid snap yields `None`/raw position when `grid.visible` is
  false and `snap_enabled` is true.
- fmt, clippy `-D warnings`, doc `-D warnings`, per-crate tests, and
  `cargo test --workspace` once at the end — green.
- `docs/roadmap.md` M18 glyph bullet updated (stability + snap-off landed;
  tangent deferred to 1-6).
- This brief committed in the PR with `## Outcome` filled.

## Outcome

*(worker fills in)*
