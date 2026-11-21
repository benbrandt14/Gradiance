# Bézier path leaf (`ShapeDef::Path`) on kurbo
Phase E (editable paths) · roadmap § "Unified plan" · status: ready (after Phase C)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 7; owner: yes to a path leaf, open paths with a width are curved rods). Docs only.

## Goal

A body's shape can be a **path**: a list of anchors, each a point with
optional in/out handles (cubic Bézier segments), closed or open. A closed
path is a solid (holes allowed, as `Polygon`); an open path carries a
`width` and is a curved rod (the stroke inflated by `width / 2`, round
caps). Colliders, meshes, borders and snapping all follow through
`polygonize`, the single discretization point.

## Read first

- `docs/sdf-geometry-decision.md` — what a new leaf costs.
- `crates/gradiance-geometry/src/shape.rs:68` — `ShapeDef`; `sdf.rs` (eval, aabb, depth); `polygonize.rs:23`; `snap_features.rs:91` (`snap_outline`).
- `docs/curvilinear-tools-decision.md` — "no leaf holds an arc edge" (this brief changes that fact; the decision about polar edges stands).
- VectorCraft `crates/geom/src/path.rs`, `anchors.rs`, `bez.rs`, `arc.rs` at `9fff48d`.

## Facts

- `kurbo` 0.13.1 is in `Cargo.lock` already (transitive). Adding it to
  `[workspace.dependencies]` as `=0.13.1` and to `gradiance-geometry` adds a
  lockfile edge, no new version; `tests/boundaries.rs` checks the exact pin
  line — follow its rule.
- `Polygon`'s field is exact (signed distance to segments + winding);
  `Capsule` is the exact rod. A flattened path reuses both: the field of a
  closed path is the polygon field of its flattening; an open path is a
  union-free "polyline capsule" (distance to the polyline minus the radius).
- Vertices are centroid-relative at authoring time (conventions); a path
  stores anchors in the same body-local frame.

## Approach

- `geometry::path`: `PathDef { anchors: Vec<Anchor>, closed: bool, holes:
  Vec<PathDef>, width: Option<f32> }`, `Anchor { p, h_in: Option<Vec2>,
  h_out: Option<Vec2> }` (None = corner side). Conversion to `kurbo::BezPath`
  and flattening at a tolerance tied to `field_cell`.
- `ShapeDef::Path(PathDef)`: eval via a cached flattening (computed once
  per shape, stored beside the derived contours, never serialized); aabb
  from control points; `validate` (≥ 2 anchors open, ≥ 3 closed; finite;
  width ≥ `MIN_SHAPE_SIZE`).
- Snap features: anchors (vertex), segment midpoints by arc length, nearest
  point on the exact curve (kurbo nearest), tangent points.
- Mirror and scale (`geometry::scale::BodyScale`, N-9) transform anchors
  and handles structurally; mirroring twice gives the scene back.
- `body.spawn_path` operation `(spawn-path x y anchors closed [width])`;
  no tool yet (E-2). `FORMAT_VERSION` bump.
- Vendor VectorCraft code only where it is copied in upstream shape.

## Out of scope

Tools (E-2), editing (E-3, E-4), width profiles (E-5).

## Done when

- Pure tests: a 4-anchor circle approximation's area within 0.1 % of πr²;
  an open path with width matches a capsule when straight; mirror twice is
  identity; proptest that flattened contours are simple and wind correctly
  (extend 6-2's suite).
- Behaviour test: a closed path body falls and rests on the ground; an open
  path rod hinged at one end swings.
- Undo matrix and loader fuzz pick up the new op/leaf (no new list).
- Gates green.

## Outcome
