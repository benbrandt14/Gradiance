# Toon bands on the bodies themselves
Phase 4b · owner note on review question `toon-lighting` (2026-11-19,
`docs/review/archive.ron`) · status: in review (#270)

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

### Landed

- **Face coordinates on every body mesh:** `geometry::extrusion` gives
  each vertex `uvs` (`face_coords`: the outline's bounding box mapped to
  `[-1, 1]`, a circle to the unit disc); `render::extrude_sync` writes them
  as `UV_0`. Affine in position, so no topology change.
- **`toon.wesl`:** `face_tilt` reads the front cap as a shallow dome over
  the face coordinates (tangents from screen derivatives, so it follows
  the body's rotation; side walls stay flat). Two uses:
  - **relief** (`RenderSettings::relief`, new; studio 0.8, blueprint and
    poster 0): the diffuse lighting uses the dome normal, so the bands step
    across the face, brightest toward the key light. The rim stays on the
    true normal; shadows are unchanged;
  - **glint** (`RenderSettings::specular`, existing, studio 0.25): a
    two-step highlight (halo inside about 14 degrees, core inside about 7)
    on a fixed dome (`toon::GLINT_DOME` = 1.0), from the strongest light
    only, in its hue.
  `ToonExtension` has a second uniform `face` (binding 101).
- **Review question `toon-highlights`** (`docs/review/questions.ron`):
  Relief + glint (default) / Glint only / Relief only / Flat (before), over
  the look scene plus `scenes/toon-highlights/polygon*.scm` (a box cut into
  a pentagon and a triangle). `expected_bodies` row: 15 each.
- **Tests:** `face_coords_span_the_outline_box` (geometry),
  `studio_lights_its_faces_as_a_dome_and_flat_looks_do_not` (render).

### Measured (lavapipe, 1280x720, review mode, studio)

Band count on each body's own front face (colour clusters of at least 3 %
of the face, dither merged; grid lines, the circle's radius line and the
cut line excluded):

| | teal box | green circle | blue pentagon |
|---|---|---|---|
| before (main, specular 0.25) | 1 | 1 | 1 |
| Flat | 1 | 1 | 1 |
| Glint only | 1 + glint | 1 + glint | 1 + glint |
| Relief only | 3 | 2 | 3 |
| Relief + glint (default) | 3 + glint | 2 + glint | 3 + glint |

- The old glint never reached a front face: with the key at azimuth 125,
  elevation 40 and a head-on view, N.H = 0.906 on a flat cap, and the
  threshold was about 0.986. Confirmed on screen (no glint pixels).
- Relief 0.6 gave a thin second band (box: 20 % of the face); 1.0 put the
  far third of each face two bands down; 0.8 is the default.
- With the sim played, the dome turns with the body and the bands stay lit
  from the world key light (checked on the fallen pentagon); a 30 degree
  perspective keeps the relief on the front caps.
- No `failed to create shader module` in the app log.

### Decisions

Three `decision: true` entries in `docs/review/archive.ron`:
`toon-face-coords` (a vertex attribute, not a uniform or new topology),
`toon-relief-looks` (studio only), `toon-glint-lights` (strongest light
only). In scope: the archived `look` question's blueprint and poster
options now pin `relief: 0.0`, which is what they showed, so
`the_look_question_shows_the_coded_presets` still holds.

### Cut

- **Bevel band** (lit rim near the outline): needs a distance to the
  outline per pixel, which the cap's vertices (all on the outline) cannot
  interpolate. It needs interior cap vertices or an SDF in the shader.
- `FORMAT_VERSION` not bumped: `relief` has a serde default.

### Regenerated

`crates/gradiance-scene/golden/v8_every_quantity.ron` and the four
`tests/it/golden/replay_*.ron` (one `relief: 0.8` line each). No UI
snapshot changed (the panels do not draw bodies).

### Not checked

- `cargo test --workspace` (CI runs it).
- A real GPU (all screenshots are lavapipe).
- The question in the interactive `--review` app by a person (the guard
  test loads every option; screenshots were taken in review mode).
- Blueprint and poster with relief turned on.

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --
-D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `cargo test -p` geometry, domain,
render, and the `it` modules review, look, settings, ui_snapshots
(`GRADIANCE_REQUIRE_GPU=1`), snapshot, replay, persistence,
op_conformance, registry_validation: all green. rustc 1.100.0 (pinned).
