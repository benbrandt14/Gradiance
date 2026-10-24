# Coordinate frames — decision pending

**Status: not yet decided.** This file exists so that the references in
`docs/units-decision.md` and `docs/roadmap.md` resolve to something honest.
It records the question and the seams already holding it open. The decision
itself is scheduled for Phase 5 of `docs/orchestration.md`.

## The question

Should authored and derived quantities be expressible in frames other than
the world frame (a body's local frame, a joint's frame, a user-defined
datum or sketch plane), and if so, where does the frame transform live?

## Why it matters now

Coordinate frames are the bridge from today's single XY simulation plane to
**multiple simulation planes** and, later, real 3D (`roadmap.md` § Horizon).
A simulation plane is, in effect, a frame that bodies are constrained to.
The decision should be made with that consumer in mind, not only CAD datums.

## Seams already held open

- **Typed quantities are frame-agnostic.** `gradiance-units` values carry a
  dimension, not a frame (`units-decision.md` § Coordinate frames).
- **`WorldScale` / `units::world`** is the one px↔SI seam; a frame transform
  composes there.
- **The grid already has a user frame.** "Align grid to body" adopts a
  body's pose as the grid's origin/rotation (`GridSettings`, roadmap V5).
  That is a frame in all but name, and snapping already works in it.
- **`DepthBand` and `INTERACTION_PLANE_Z`** are the current single-plane
  assumptions; new code should read them from one place rather than
  hardcode z.

## Inputs to read when deciding

- `docs/units-decision.md` (the catalog and the `WorldScale` seam).
- PR #208 (draft): avian2d → bevy_rapier3d migration with groundwork for
  multiple simulation planes. Parked; read for its frame model, don't merge.
- The curvilinear-tools question deferred from M18 (do tools operate in grid
  coordinates?) — the same question at the interaction layer.
