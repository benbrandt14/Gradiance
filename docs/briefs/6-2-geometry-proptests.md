# Geometry property tests
Phase 6 (testing & diagnostics) · `docs/roadmap.md` § C (insta + approx +
proptest) · status: ready (after 6-1)

Written by the orchestrator, 2026-11-20.

## Goal

Property tests in `gradiance-geometry` (pure math, no app) for the shape
pipeline that every derived consumer reads:

1. `polygonize` of random leaves and random CSG trees: contours are
   closed, have consistent winding (outer CCW, holes CW), have no
   self-intersection, and their area matches the analytic area of the
   leaves within a stated tolerance.
2. CSG identities: `A ∪ A = A`, `A − A = ∅`, `A ∩ B ⊆ A`, area of union
   ≤ sum of areas; the tolerance is stated.
3. Later shape math from Phase N: the sketch stroke to shape
   (`geometry::sketch`), the chain path split (`geometry::chain`), the
   width profile (`geometry::profile`), the submerged area clip (N-7) and
   `mirror_shape` (N-9): mirroring twice is the identity, the submerged
   area of a fully inside body equals its area, a profile width is never
   negative.

## Facts

- `proptest` is a dev-dependency of `gradiance-kernel` and
  `gradiance-units` and of the root package; adding it to
  `gradiance-geometry`'s dev-dependencies uses the same workspace version
  (no lockfile change beyond the dependency edge line, if any; check
  `cargo tree` first and record it).
- `geometry::polygonize` is the single discretization point.

## Decide (best estimate, record in `archive.ron`)

- Case counts that keep `cargo test -p gradiance-geometry` under 30 s.
- Tolerances, each with its reason (cell size, curve segments).

## Out of scope

Changing polygonize quality or speed, except to fix a bug a property
finds (fix it with a regression case and list it in the Outcome).

## Done when

- The properties above run in `cargo test -p gradiance-geometry` and pass.
- Every failure found is fixed or recorded as a known limit with a
  minimised case committed as a regression test (marked as such).
- Gates green; Outcome filled.

## Outcome

*(worker fills in)*
