# Live corners: round, inverted round and chamfer per corner
Phase H · roadmap § "Unified plan" · status: ready (after E-3)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 15). Closes M21 "fillets of existing corners of one body, chamfers". Docs only.

## Goal

With a box, polygon or path selected, a small dot sits inside each corner;
dragging one rounds that corner (and, with all dots shown together, all of
them); a modifier or the inspector picks round, inverted round or chamfer.
A radius field and a per-corner list live in the inspector. One drag is one
command.

## Read first

- `docs/interaction-feel.md` — row **Illustrator**.
- VectorCraft `crates/geom/src/corners.rs`, `crates/doc/src/corners.rs`, `crates/tools/src/corners.rs` at `9fff48d`.
- Review archive `fillet-bulge` (the smooth-min bulge is accepted; corners are a different, exact operation).

## Facts

- A rounded box is an exact SDF (`box(p, half - r) - r`); per-corner radii
  for polygons are exact as outline geometry (arc between the trimmed sides),
  which a `Path` holds natively.
- Corner data must survive E-3 edits: corners are stored by anchor index.

## Approach

- Feel: **Illustrator**. `Box` gains `corners: [Corner; 4]`; `Polygon`
  and `Path` gain `corners: Vec<Corner>` (`Corner { kind, radius }`,
  default sharp). The outline with cut corners is computed in
  `geometry::corners` (vendored from VectorCraft where copied) and used by
  the field (polygon/path) or the exact rounded-box SDF.
- Widgets: dots placed along each corner's bisector at the current radius,
  clamped to the shorter side.
- `body.corners` operation. `FORMAT_VERSION` bump.

## Out of scope

Corners of CSG results (bake first). Smooth subtract/intersect.

## Done when

- Pure tests: rounded box area `wh - (4 - π) r²`; chamfer area; radius
  clamps at the shorter side; corners survive moving an anchor (E-3).
- Behaviour test: a rounded-corner box rolls further on a slope than a
  sharp one (sanity, not a calibration).
- Gates green; screenshot.

## Outcome
