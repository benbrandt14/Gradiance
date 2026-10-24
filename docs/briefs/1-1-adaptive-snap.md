# Snap to the grid you can see
Phase 1 · M18 · "snap points provably on the grid at every adaptive zoom
level (3.3, 2.5)" · status: ready

## Goal

The cursor snaps to exactly the grid lines that are drawn, at every zoom
level and in every grid system. Today the renderer coarsens/refines the
drawn spacing by powers of two while snapping uses the authored spacing raw:
zoomed out, the cursor snaps to invisible points between drawn lines; zoomed
in, snapping is coarser than the visible grid.

## Read first

- `crates/gradiance-render/src/grid.rs` — `draw_grid`, `adaptive_spacing`,
  the `MIN/MAX_SCREEN_SPACING` band and its test.
- `crates/gradiance-interaction/src/snap.rs` — `update_snapped_cursor`,
  the grid-snap block at the end.
- `crates/gradiance-geometry/src/snapping.rs` — `cartesian/isometric/polar_snap`.
- `crates/gradiance-interaction/src/camera.rs` ~L104 — `CameraScale`, the
  single screen↔world sizing authority.

## Facts (found by the orchestrator, 2026-10-24)

- `render/grid.rs:31` computes `adaptive_spacing(grid.spacing, ortho.scale)`;
  the function is private to render (`grid.rs:94`). Band constants are
  `grid.rs:10–11` (24 and 96 screen px).
- `interaction/snap.rs:142–153` snaps with `grid.spacing` directly. The same
  system already has `cam_scale: Res<CameraScale>` (`snap.rs:69, 82`).
- `render` depends on `interaction`, but the shared math belongs in
  `geometry`, which both reach and which is the designated home for
  testable math (CLAUDE.md conventions).
- **Second-order mismatch:** `draw_grid` reads `ortho.scale` and returns
  early under a perspective projection (`grid.rs:23`), while `CameraScale`
  is defined for both projections. So the two sides use different scale
  sources even before the doubling. *Unverified:* whether some other system
  draws the grid under perspective — check before assuming the grid is
  simply hidden there. If it is hidden, snapping under perspective is also
  "snapping to invisible points."

## Approach

1. Move `adaptive_spacing` (and its band constants) into
   `gradiance-geometry::snapping` as a pub function, e.g.
   `adaptive_step(base, world_per_px) -> f32`. Pure math, unit-tested in
   the geometry crate (`cargo test -p gradiance-geometry`, no app link).
2. Both `draw_grid` and `update_snapped_cursor` call it with the **same**
   scale source — `CameraScale`, not `ortho.scale`.
3. Snap with the adaptive step in all three grid systems (polar uses it for
   ring radius; angular divisions are unchanged).
4. Keep the existing render test's intent (on-screen spacing stays in the
   band) — move or re-home it with the function.

Invariants touched: none of the five. `GridSettings` stays workstation
config; nothing new is authored or persisted.

Future-intent note (`docs/orchestration.md`): this introduces no new
single-plane assumption — keep it that way; the step is a 1-D scalar.

## Out of scope

Major/minor line styling (brief 1-2 reuses this function), axis-lock basis,
snap glyphs, curvilinear coordinates, any UI change.

## Done when

- A geometry-crate test: for a sweep of camera scales, every snapped point
  is an integer multiple of the drawn step from the grid origin (in the
  rotated basis), and the step's on-screen size is within the band.
- An integration test in `tests/it/` that sets a zoomed-out `CameraScale`
  and asserts the snapped cursor lands on the adaptive step, not the base.
- All four gates green; `docs/roadmap.md` M18 first bullet marked landed
  with a one-line note; `docs/state.md` updated.

## Outcome

*(worker fills in at handoff)*
