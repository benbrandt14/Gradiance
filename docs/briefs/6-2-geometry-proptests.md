# Geometry property tests
Phase 6 (testing & diagnostics) · `docs/roadmap.md` § C (insta + approx +
proptest) · status: in review

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

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-6-2`.

**Landed:** `crates/gradiance-geometry/src/proptests.rs` (a `#[cfg(test)]`
module, so it runs in the crate's own unit-test binary and never links the
app). `proptest` is now a geometry dev-dependency at the workspace version
(`cargo tree` had it at 1.11.0 already; `Cargo.lock` gains the one
dependency-edge line). Generators: boxes, circles, capsules and star-shaped
polygons with an optional hole, placed at random; CSG trees of every
operator (smooth union included) up to three levels; trees with
half-planes for the mirror; random-walk and loop strokes. Fifteen
properties (thirteen generated, two regressions):

1. `polygonize`: a bare leaf is exact (a circle or cap within the inscribed
   48-gon loss, never above the curve); a placed leaf contours to one
   well-formed component of its analytic area and perimeter; a random tree
   contours to well-formed components (rings closed, outlines CCW, holes CW
   and inside, no two edges of any rings touching) that classify every
   probe point more than 1.5 cells from the boundary as the field does.
2. CSG: `A ∪ A = A`, `A ∩ A = A` bit for bit, `A − A = ∅` and `A ⊕ A = ∅`
   exactly, for random trees; for two placed leaves `A ∩ B ⊆ A, B` (every
   vertex), `|A ∩ B| ≤ min`, `max ≤ |A ∪ B| ≤ |A| + |B|`,
   inclusion-exclusion, `|A ⊕ B| = |A ∪ B| − |A ∩ B|`,
   `|A − B| = |A| − |A ∩ B|`; `UnionField` equals `sdf::eval` bit for bit.
3. Phase N: a stroke makes a valid shape or is under the minimum length; a
   closed one is a simple CCW polygon about its centroid, an open one stays
   within the segment and depth caps, its centre line lies `radius` deep
   in it, and it contours as one piece. A chain split has `n + 1` points
   from the exact first to the exact last, `n = round(length / link)`
   clamped, no link longer than its share, equal links on a straight path.
   A width profile is never negative or non-finite, the same for `d` and
   `−d`, at most its maximum, at most the outline's diameter. A body wholly
   inside a box, 48-gon or concave L tank, or under a sea, is wholly
   submerged (area and centroid); far away, none; below plus above a
   surface is the whole. Mirroring twice is the tree bit for bit (the same
   field where it holds a half-plane); mirroring once reflects the field
   and keeps validity and area.

Each tolerance is stated in the property's doc comment with its reason
(cell size, chord sag, simplification, `f32` shoelace rounding).

**Bugs found and fixed** (each a minimised regression test that failed
first, marked `regression_` in the module):

1. `sdf::UnionField::eval` skipped a leaf whose bounding box held the point
   once the best value so far was negative (`0 ≥ best + margin`), so the
   value was not the tree's field. The sign was always right, so contours
   did not change. It now culls only outside the box.
   `regression_union_field_never_culls_a_leaf_whose_box_holds_the_point`.
2. `contour::simplify_ring` tested each dropped vertex only against the
   chord to its next neighbour, so a run of dropped vertices drifted: a
   capsule's straight side ran 0.48 m into both caps, 7.5e-4 m inside the
   curve against a tolerance of 3.2e-4 m, and the area came out 0.63 %
   short. It now keeps a vertex unless the whole dropped run stays within
   the tolerance of the new chord.
   `regression_contour_simplification_stays_within_its_tolerance`.

**Known limits** (by design, tested as such): a contour chamfers corners
and drops slivers under `4 · cell²` (the tolerances above); a tree with a
half-plane mirrored twice gives the same field, not the same tree.

**Run time:** `cargo test -p gradiance-geometry`: 2.1 s of test time for
the lib binary (168 tests, 3584 generated cases; fixed seed, no
failure-persistence file, nothing written to disk). Explored six other
seeds at 2000 cases each (about 17 s each): all passed after the fixes.

**Decisions** (six, in `docs/review/archive.ron`): the dependency, case
counts and seed, the tolerances, fixing the simplification drift, fixing
the cull, and testing the half-plane mirror by field.
