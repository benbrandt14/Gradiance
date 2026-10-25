# Snap to the grid you can see
Phase 1 · M18 · "snap points provably on the grid at every adaptive zoom
level (3.3, 2.5)" · status: done (PR #212)

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

**Landed (PR #212).** `geometry::snapping::adaptive_step(base, world_per_px)
-> Option<f32>` replaces render's private `adaptive_spacing`, with the band
as `GRID_MIN/MAX_SCREEN_STEP`. `draw_grid` and `update_snapped_cursor` both
call it with `CameraScale`. The two grid line families are shared constants
(`CARTESIAN_/ISOMETRIC_LINE_DIRECTIONS`), so the renderer and the snapper
work from one description of the grid. The band test was re-homed in
geometry and extended into the acceptance sweep
(`snapped_points_lie_on_the_drawn_grid_at_every_zoom`). Integration tests
`zoomed_out_*`, `zoomed_in_*` and `zero_grid_spacing_snaps_to_nothing` are in
`tests/it/interaction.rs`.

**Found beyond the brief. The next session would otherwise rediscover these:**

- **Isometric snapping was off-grid at every zoom, not just adaptive ones.**
  The snapper used lattice vectors of length `spacing`. The renderer spaces
  lines `spacing` apart *perpendicular to the lines*, so their
  intersections are `spacing / sin 120°` (about 1.155×) apart. Snapping
  now solves for drawn-line intersections. The old geometry and
  integration tests pinned the wrong lattice; they now check the point
  against the drawn lines.
- **Directions, not angles.** `from_angle(π/2)` is not exactly `(0, 1)` in
  f32, which put an unrotated Cartesian snap at `400.00003`. The family
  constants are therefore unit vectors.
- **Perspective (the brief's unverified fact):** `draw_grid` is the only
  grid drawer, and it returned early under perspective while snapping stayed
  active. The grid now draws under both projections, centred on
  `CameraRig.focus` rather than the camera's xy (identical in the flat
  orthographic view; correct when orbited). *Orchestrator call:* this is a
  visible change. The alternative was disabling grid snap under
  perspective. Known limit: coverage is still a disc of `800·scale` around
  the focus, so a strongly tilted view can show the grid's edge before the
  horizon. Sizing it from the frustum is not done.
- **Same frame, same value.** Snapping runs in `PreUpdate`, `apply_camera_rig`
  in `Update`. `draw_grid` is now ordered `.before(InteractionSet)`, so both
  read the same `CameraScale` on the frame a zoom crosses a doubling.
- **Degenerate spacing (caught by the cold review).** The old function
  clamped the base to `EPSILON`. The spacing field has no lower bound, so a
  base of 0 hung the polar ring loop. `adaptive_step` returns `None` for a
  non-positive or non-finite base: no grid is drawn and no grid snap.
- **Existing tests encoded the defect.** At the default `CameraScale(1.0)`
  a 100-unit grid draws every 50, so two integration tests asserted
  snapping to points that were not drawn. They now set `CameraScale(2.0)`,
  where the drawn step equals the base.
- **Single-crate builds rebuild Bevy.** `cargo test -p gradiance-geometry`
  resolves a different Bevy feature set from the workspace, so the first
  `-p` run in a fresh container compiles a second Bevy (about 20 minutes
  here). Start it early, or in parallel with the workspace build.

**CI fix (outside the brief, needed to read this PR's CI):** the "Annotate
clippy failures" step ran on `failure()` of *any* step, so a Format failure
made it read a `clippy.txt` that was never written. It is now
`failure() && steps.clippy.outcome == 'failure'`, and Format re-emits each
unformatted file as an annotation.

**Unchanged on purpose:** the major-line every-5th styling counts adaptive
lattice steps, so majors fall on different world positions as the step
doubles. That is for brief 1-2.
