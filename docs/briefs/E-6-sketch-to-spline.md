# Sketch strokes fitted to splines
Phase E · roadmap § "Unified plan" · status: ready (after E-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 12). Docs only.

## Goal

The sketch tool's stroke becomes **one path** fitted by least-squares
cubics (an open rod of the stroke width, or a closed solid when the stroke
closes), instead of an RDP polyline of up to 24 capsules. The outline is
smoother, has far fewer pieces and is editable with E-3. Not shape
recognition (owner-declined): the fit follows the stroke, it does not
replace it with a box or a circle.

## Read first

- `crates/gradiance-geometry/src/sketch.rs` — today's RDP + capsule union (N-6).
- VectorCraft `crates/pathops/src/fit.rs` at `9fff48d`.

## Facts

- N-6 caps segments at 24 to keep `MAX_CSG_DEPTH`; a single `Path` leaf has
  no depth cost.
- `body.sketch` and Adjust Last on width must keep working.

## Approach

- Vendor `fit.rs` into `gradiance-geometry/src/vendor/vectorcraft/` with a
  thin adapter; tolerance a fraction of the stroke width.
- `sketch::stroke_shape` returns `ShapeDef::Path`; the RDP path is removed.

## Out of scope

Shape recognition (declined 2026-11-21).

## Done when

- Pure test: a sampled circle stroke fits within 2 % radius error with ≤ 8
  anchors; a straight stroke fits as one segment.
- N-6's behaviour tests pass unchanged or are updated with reasons.
- Gates green.

## Outcome
