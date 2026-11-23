# Shape from an image
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("geometry from opaque image") · status: ready

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

File ▸ Import image as body makes one body whose outline follows the
image's opaque pixels. It collides, cuts and merges like any body.

## Read first

- `docs/sdf-geometry-decision.md` — what a new leaf costs.
- `crates/gradiance-geometry/src/polygonize.rs` — the single discretization point.
- `crates/gradiance-geometry/src/shape.rs:70` — `ShapeDef` leaves.

## Facts

- CLAUDE.md: never contour a field outside `polygonize`. So the image
  becomes a field leaf, not a polygon traced elsewhere.
- Option A: a sampled-grid SDF leaf (`ShapeDef::Grid`, signed distance
  per cell, bilinear eval). Option B: trace once at import with
  `polygonize` over a temporary grid field and author a `Polygon`.
  B adds no leaf; A keeps the image editable later. Worker decides and
  records it.

## Approach

- Alpha threshold 0.5; distance transform at import (pure, in geometry).
- `scene.import_image` (`(import-image path x y width)`), one `spawn-body` step.
- Optional: the image as the body's texture (S-13) in the same step.

## Out of scope

Colour-segmented import (one body per colour). Vector file import.

## Done when

- Pure test: a disc image imports with area within 2 % of πr².
- Gates green; `FORMAT_VERSION` bumped if option A.

## Outcome

Done (2026-11-23).

- **Option B** (decision `image-import-polygon`): the image is traced
  once, at import, into a plain `Polygon`. No new shape leaf, so no format
  change and `FORMAT_VERSION` stays 8. The image is not editable as a
  field later; re-import it instead.
- **Tracing:** `geometry::alpha_mask::AlphaMask` (bilinear alpha, solid at
  0.5) gives the field; `polygonize::polygonize_alpha` contours it with
  the shared `contour_components`, keeps the largest part and simplifies
  each ring to at most 256 vertices. Tracing stays in `polygonize`, the
  single discretization point. The field is the alpha level, not a
  distance transform: only its zero set is used.
- **Operation:** `scene.import_image` (`(import-image path x y width)`),
  one `spawn-body` step. The body shows the image (an S-13 texture) where
  the pixels were. An empty path is a file dialog: File ▸ "Import image
  as body…" (one metre wide, at the origin).
- **Asset:** `assets/images/import-disc.png` (64 px, a 28 px disc) for the
  operation example and the test.
- **Tests:** pure, a 200 px disc traces within 2 % of pi r squared
  (`polygonize::tests::a_disc_image_traces_to_its_area`);
  `tests/it/image_import.rs` (the 64 px disc within 3 %, the image centre
  where asked, one undo step, a missing file spawns nothing); the undo
  matrix and the conformance suite.

