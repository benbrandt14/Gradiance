# Tangent constructors: circle tangent to two outlines, three-point arc, smooth bridge
Phase H · roadmap § "Unified plan" · status: ready (after C-3, E-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 16). Docs only.

## Goal

Three CAD constructors as circle/path tool options:

1. **Tangent circle**: click one outline, then another; the circle tangent
   to both (at the first click's place) is drawn; `Shift` fixes the radius.
2. **Three-point arc**: two ends, then a third point on the arc; commits an
   open path rod (E-1) — also the "arc-on-a-ring constructor" of
   `docs/curvilinear-tools-decision.md` option 3 when the three points are
   on a polar ring.
3. **Smooth bridge**: two circles joined by one smooth closed outline (a
   link/plate between two pivots), committed as a path body.

## Read first

- `docs/interaction-feel.md` — row **CAD**.
- `crates/gradiance-geometry/src/construct.rs` — `three_point_box` (1-6).
- VectorCraft `crates/tools/src/addons/tangent_circle.rs`, `three_point_arc.rs`, `crates/pathops/src/addons/pathtools/bridge.rs` (`smooth_bridge`) at `9fff48d`.

## Approach

- Feel: **CAD**. Pure functions in `geometry::construct` (`circles_tangent_to`,
  `three_point_arc`, `smooth_bridge`), ported with VectorCraft's tests.
- Tool options on `ToolDefaults` (as `box_three_point` is), each with an
  operation.

## Out of scope

Constraints that keep tangency after an edit (Horizon, CAD kernel).

## Done when

- Pure tests: tangent circle touches both outlines within 1e-4 m; an arc
  through three ring points has the ring's radius; bridge outline is
  tangent-continuous.
- Gates green.

## Outcome
