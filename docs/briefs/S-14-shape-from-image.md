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
