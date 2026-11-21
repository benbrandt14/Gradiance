# A derived contour per body
Phase Q (follow-ups) · N-6 Outcome, "Learned": "body borders re-contour
every body every frame … a per-body derived `Contours` cache would remove
the per-frame cost for every CSG body" · status: done

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

A body's shape is contoured once per shape change, not once per frame.
Today `draw_body_borders` calls `polygonize` for every body every frame,
and the selection outline and the scale handles do it for every selected
body every frame; a CSG body (a sketch, a fillet, a cut) pays its full
contour cost each time (about 3 ms for a 24-leaf sketch at dev opt-level
1, N-6). After this brief a derived `BodyContours` component, built beside
the collider, is what those per-frame readers use. Frame time with 50
sketched bodies on screen goes down by at least the contour cost
(measured, stated in the Outcome).

This completes the original SDF plan: "one derived `Contours` cache per
body" (`docs/sdf-geometry-decision.md:60`). `polygonize` stays the single
discretization point: the cache stores its output.

## Read first

1. `docs/sdf-geometry-decision.md` (the cache in the plan, the single
   discretization point).
2. `CLAUDE.md` invariant 5 (derived state: rebuilt by `Changed<>` sync,
   never saved, never in undo, never read by commands).
3. `crates/gradiance-physics/src/body_sync.rs:105-130` (`sync_colliders`,
   the one place a shape is polygonized for derived physics data).
4. `crates/gradiance-render/src/decorations.rs:20-50`
   (`draw_body_borders`).

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- `polygonize(shape) -> Contours`
  (`crates/gradiance-geometry/src/polygonize.rs:22`);
  `polygonize_components` (`:69`) is what `sync_colliders` calls once per
  shape change (`body_sync.rs:114-130`), feeding the collider,
  `AirProfile` and `SubmergeOutline`.
- Per-frame `polygonize` calls on live bodies:
  `crates/gradiance-render/src/decorations.rs:36` (every body with a
  border), `crates/gradiance-interaction/src/indicators.rs:110` (each
  selected body), `crates/gradiance-interaction/src/tools/handles.rs:134`
  (each selected body), `crates/gradiance-render/src/debug_viz.rs:104`,
  `:127` (debug overlays, when on).
- Calls on gesture ghosts (`crates/gradiance-interaction/src/tools/select.rs:249`,
  `:271`) contour a copied shape during a drag; they are not live bodies
  and may stay as they are.
- `extrude_sync.rs:23`, `:85` contour on mesh rebuild (change-driven),
  `crates/gradiance-physics/src/fields.rs:71` per field source; check
  each and say in the Outcome whether it moves to the cache.

## Approach

- `BodyContours(Arc<Contours>)` (or the components list, if a reader needs
  pieces), a derived component in physics beside the collider, inserted by
  `sync_colliders` from the contours it already computes (the union of
  the components, or one extra `polygonize` there, whichever matches
  today's border output exactly).
- Readers query `&BodyContours` instead of calling `polygonize`. A body
  without the component yet (the frame it spawns) draws nothing that
  frame, or falls back to `polygonize`: pick one and say which.
- `draw_body_borders` also stops allocating a `Vec` per ring per frame
  (reuse a `Local` buffer, or gizmo `linestrip` from an iterator).
- No saved, undo or command change. No change to how contours are made.

## Tests

- A unit or `tests/it` test: the cached contours equal `polygonize(shape)`
  after spawn and after a shape edit (scale, cut, merge), and the
  component is absent from the saved scene.
- A headless timing check in the Outcome (not a CI assertion): frame time
  of `Update` with 50 sketched bodies before and after.
- The `ui_snapshots` and the existing render tests pass unchanged.
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-physics`,
  `-p gradiance-render`, `-p gradiance-interaction`, the touched
  `--test it` modules.

## Out of scope

- Changing `polygonize`, its resolution or the sample budget.
- Mesh extrusion caching beyond what is listed above.
- Any visual change: borders and outlines look the same.

## Outcome

Done (2026-11-21); most of it had already landed in P-1 (#300).

- P-1 added the cache this brief asks for: `geometry::polygonize::ShapeContours`,
  built in `sync_colliders` from the components it already computes, and
  read by borders, the selection outline, snapping and the debug overlay.
  This brief moved the one remaining per-frame contour of live bodies:
  `tools::handles::selection_box` (the scale handles and the select tool's
  box) now reads `ShapeContours`. It contours once only on the frame a body
  spawns, before its cache exists. It now spans every component (it used to
  use the largest one only).
- Left as they are: gesture ghosts (`select.rs` previews of copied shapes),
  `extrude_sync` (change-driven mesh rebuild), `fields::sync_field_mass`
  (change-driven), `pack.rs` (once per run), `array_tool` (during a
  gesture).
- Test: `tests/it/contour_cache.rs` (the cache equals
  `polygonize_components` after spawn, scale, cut and merge; never saved).
- No frame-time measurement: the remaining change only affects scenes with
  a selection. P-1 measured the main win.
