# Steady snap glyphs on curves
Phase 1 · M18 · "Snap glyph stability, tangent glyph, snap-off-when-grid-
hidden (3.4)" — stability + snap-off · status: done

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

**Landed — analytic features, not hysteresis.**

- **Cause confirmed.** `circle_outline_snap_is_steady_along_the_curve`
  (`tests/it/interaction.rs`) sweeps 1440 steps around an r = 50 circle,
  2 px outside, at 1 world/px. On the pre-fix snapper (first commit of the
  PR, CI red) it fails. The 48 tessellation vertices and 48 midpoints
  alternate every ≈3.3 px along the outline, all inside the 12 px capture
  radius, so the winner flips vertex / midpoint 96 times per revolution.
  Edge never wins, because a vertex or midpoint is always within ~1.6 px
  (offline replay of the old candidate set). With the fix: exactly 8
  changes, `Edge` (exactly on the circle) except a `Vertex` run at each
  quadrant.
- **Fix at the source:** `geometry::snap_features` (new module, not
  appended to `snapping.rs` so the parallel briefs don't conflict).
  `snap_outline(&ShapeDef) -> SnapOutline` returns `Circle {center,
  radius, rotation}` for a bare circle or a circle under any nesting of
  `Placed`, else `Contours(polygonize(shape))`. `circle_quadrants()` and
  `nearest_on_circle()` are the features. `snap.rs` matches on it: a circle
  offers its quadrants as `Vertex`, the exact nearest point as `Edge`, no
  midpoints; everything else keeps the old ring loop. Centre snap is
  unchanged (body origin). The edge tie bias became a named const
  `EDGE_TIE_BIAS`.
- **Why not hysteresis:** the candidates were the bug, since none of
  them lie on the true curve. A sticky kind would still have offered
  points on the tessellation and still jumped between them. Hysteresis
  would also need cross-frame state in a system that is pure per-frame
  today.
- **Snap-off pinned:** `grid_snap_is_off_while_the_grid_is_hidden`. The
  behaviour already existed (`snap.rs` grid gate); no code change.
- Roadmap M18 glyph bullet updated.
- **Gates:** run locally: fmt, clippy `-D warnings`, doc `-D warnings`,
  `cargo test -p gradiance-geometry`, and the `tests/it` interaction
  module (14/14). **`cargo test --workspace` was run by CI, not locally**:
  the shared container was down to ~3 GB of disk (orchestrator's call).
  rustc 1.97.0 = `RUST_PIN`.

**Not done / for later.**

- CSG shapes that contain circles (a body with a bite, a smooth union)
  still snap on contoured vertices, which come from the
  `FIELD_CELLS` field contour: dense along arcs, same flicker class. The
  fix there is per-leaf features clipped to the CSG boundary (keep a leaf
  feature where `sdf::eval(root) ≈ 0`). That is a separate piece of work;
  1-5 (same engine) is the natural home.
- **For 1-6 (tangent snap):** `SnapOutline::Circle` already gives the
  centre and radius in the shape frame. A tangent point from a reference
  `q` outside the circle is `c + r·rot(±acos(r/|q−c|))·(q−c)/|q−c|`.
  Compute it in the shape frame (inverse affine, the same as the edge
  projection in `snap.rs`) and map back with `world_point`. A
  `SnapKind::Tangent` row then needs a glyph arm in `draw_snap_indicator`
  (exhaustive match: the compiler will find it). The reference point has
  to reach `update_snapped_cursor`, likely via a small resource the tool
  sets at the first click, like `SnapExclusions`.

