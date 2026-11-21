# Snap by the grabbed outline point; a line tangent to two outlines
Phase C · roadmap § "Unified plan" · status: ready (after C-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 3). Docs only.

## Goal

1. A body dragged by a press on or near its outline snaps **by the point of
   the outline nearest the press**, not by the pressed pixel beside it.
2. A segment drawn by the rod, spring or chain tool (and later the pen),
   starting on a round outline and ending near another, can snap both ends
   onto a **common tangent** of the two outlines — the belt-over-two-pulleys
   and rod-on-two-wheels gestures.

## Read first

- `docs/interaction-feel.md` — row **CAD**.
- `crates/gradiance-geometry/src/snap_features.rs` — `SnapOutline`, `tangent_points` (:204), `capsule_tangent_points`.
- VectorCraft `crates/tools/src/guides/snap.rs` `hold_on_path`; `guides/bitangent.rs`; `crates/pathops/src/addons/pathtools/tangents.rs` (`common_tangents`) at `9fff48d`.

## Facts

- Circles and capsules have exact snap features today (1-4, 1-7); the
  tangent snap is point-to-circle only.
- Common tangents of two circles are closed-form (4 lines); for convex
  outlines VectorCraft samples and bisects.

## Approach

- Feel: **CAD**. Pure `snap_features::common_tangents(a, b) -> Vec<(Vec2, Vec2)>`
  for circle/capsule pairs exactly, convex polygons by support functions.
- The start end slides along the first outline during the drag; the snap
  kind is `Tangent` with the C-1 label.
- Grip point: the move gesture records the outline point at press
  (`MoveAlignment`), and alignment and grid snaps use it.

## Out of scope

A belt/chain that wraps pulleys (a later tool on T-4); concave outlines.

## Done when

- Pure tests: four tangents of two separate circles, two of nested-touching,
  none of concentric; capsule pair.
- Behaviour test: a rod drawn from wheel A to wheel B lands tangent to both
  (distance from each centre = radius ± 1e-4 m).
- Gates green.

## Outcome
