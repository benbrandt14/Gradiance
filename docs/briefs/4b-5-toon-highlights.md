# Toon bands on the bodies themselves
Phase 4b · owner note on review question `toon-lighting` (2026-11-19,
`docs/review/archive.ron`) · status: ready

Written by the orchestrator, 2026-11-19, at the ingest of six review
answers.

## Goal

The owner picked the new default lights and added: "Ideally there would
still be bands (for like, specular highlights basically) on the objects
themselves not just their shadows." Today a flat front face gets one
light value, so the bands show only where shadows fall. A body should
show stepped light on its own face, the way a toon specular highlight
or a lit edge does.

The bar: a box, a circle and a polygon in the studio look each show at
least two bands on their own front face in the default view, and the
owner picks the variant in review mode.

## Read first

- `docs/briefs/4b-3-art-direction.md` Outcome ("Toon bands visible"): the
  winding fix, the key/fill/ambient levels and the banded back plane.
- The live toon material and its shader (`crates/gradiance-render/src/
  toon.rs`, the `.wesl` shader it loads; shaders are WESL since D-1).
  `RenderSettings::specular` (studio 0.25) is a "quantized specular
  glint". Measure first whether it is visible at all on a front face.
- `crates/gradiance-domain/src/theme.rs` (`Look::render`, `Look::lighting`).
- `docs/orchestration.md` § Review mode (how to ask).

## Facts (orchestrator, 2026-11-19, main `92ab302`)

- An extruded body's front cap is flat: one normal, so N·L is constant
  and one band covers the face. Only the side walls (seen when tilted)
  and the shadows change band.
- Studio has `specular: 0.25`; Blueprint and Poster have 0.
- *Unverified:* whether the glint ever lands on the front cap with the
  current key direction and an orthographic-like view. Measure it.

## Approach

1. Measure on lavapipe (`tools/screenshot/`): screenshot the review scene
   `docs/review/scenes/look/scene.scm` in studio; report the band count
   on each body's front face.
2. Build two or three candidate variants behind `RenderSettings` fields,
   for example:
   - a stepped specular highlight that does land on the front face
     (half-vector from a view direction, or a point-light term);
   - a bevel band: normals bent near the contour so each face gets a
     lit rim band and a core band;
   - a stepped gradient across the face toward the key light.
   Pick the ones that need no new mesh data first.
3. Ask: add a review question `toon-highlights` with the variants as
   options over the look scene (settings overrides; expected bodies in
   `tests/it/review.rs`).
4. Default to your best variant now; the owner's answer can change it.
5. Snapshot tests (U-5) regenerate where the default changes.

## Out of scope

New looks, 2D lighting crates, SSAO, the panel theme, mesh topology
changes beyond normals.

## Done when

- Each candidate is a setting; the default shows two or more bands on a
  front face (screenshot evidence in the PR).
- The review question exists and the review guard passes.
- Gates green (fmt, clippy, doc, boundaries, targeted tests; CI for the
  workspace). Outcome filled.

## Outcome

*(worker fills in)*
