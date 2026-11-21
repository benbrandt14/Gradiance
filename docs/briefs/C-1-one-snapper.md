# One snapper for every tool (alignment while drawing, construction guides, labels)
Phase C (construction & measurement) · roadmap § "Unified plan" · status: ready (after Phase S)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 1). Docs only.

## Goal

Every creation tool (box, circle, polygon, rod, chain, sketch ends, spring,
connectors, laser aim, script tools) snaps its points through **one**
snapper that also offers what the select tool's move already has:
alignment with nearby bodies' edges and centres (brief 1-5), plus
construction-angle guides from the previous point (0/90°, 45°, and an
optional 30/60° set) and a short text label beside the snap glyph
(`vertex`, `midpoint`, `centre`, `edge`, `tangent`, `grid`, `align`). This
closes M18's "alignment during creation tools is still open".

## Read first

- `docs/interaction-feel.md` — row **CAD**; shared invariant 4 (one snap language).
- `crates/gradiance-interaction/src/snap.rs` — `SnapKind` (:26), `SnappedCursor`, `DraftReference`, `update_snapped_cursor` (:149).
- `crates/gradiance-interaction/src/tools/move_align.rs` and `crates/gradiance-geometry/src/align.rs` — the move-time alignment (1-5).
- VectorCraft `crates/tools/src/guides/snap.rs` (`DrawSnap`, `Leave`, `construction_angles`) and `guides/targets.rs` at `9fff48d`.

## Facts

- Creation tools already read one `SnappedCursor`; the gap is that it knows
  only grid and object features, not alignment or construction angles.
- `DraftReference` (snap.rs:75) already carries "the point a draft leaves
  from" for tangent snaps — VectorCraft's `Leave` is the same idea.
- VectorCraft gathers targets once per document state and looks them up by
  bisection on sorted coordinates; Gradiance's snap collider scan was made
  paused-and-unchanged-gated in P-1. *Unverified* whether the per-move cost
  needs the sorted index at today's scene sizes; measure first.

## Approach

- Feel: **CAD**. Pure math in `gradiance-geometry` (a `construct_angle`
  projection beside `align`), ECS in `interaction::snap`.
- Extend `SnapKind` with `Align` and `Construction`; extend the priority
  order: exact object features > tangent > alignment > construction angle >
  grid. One function picks; every tool reads it.
- Construction angles are workstation config on `SnapConfig` (default
  "90° & 45°"), with a Settings > Grid & Snap row and a `snap.*` operation.
- Labels: one short ASCII word per kind, drawn by the overlay beside the
  existing glyph; a `SnapConfig::labels` switch (on).
- Port VectorCraft's target gathering only if measurement shows the need;
  if ported, vendor it per `docs/unified-surface-plan.md` § 6.

## Out of scope

Distance guides (C-2), grip-point snapping and two-circle tangents (C-3),
typed lengths (C-6), rulers (B-4).

## Done when

- Pure tests: construction projection onto 45° from a reference; priority
  order (a vertex beats an alignment within the same radius).
- Behaviour test: drawing a box whose corner lines up with a neighbour's
  top edge lands exactly on it (both with the grid shown and hidden — a
  hidden grid still snaps nothing, 1-4).
- Every creation tool is covered by one table-driven test that it reads the
  shared snap.
- fmt, clippy, tests, doc gate green; screenshot of labels on the PR.

## Outcome
