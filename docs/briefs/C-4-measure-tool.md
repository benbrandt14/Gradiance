# Measure tool
Phase C · roadmap § "Unified plan" · status: ready (after C-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 4). Reverses the 2026-11-19 rejection (owner, 2026-11-21). Docs only.

## Goal

A Measure tool: drag between two snapped points to read distance, angle and
dx/dy (Shift: angle steps), shown until the next drag; hover a body to read
its outline's perimeter, area, and, near a curved part, the radius of
curvature at the pointer; hover a joint to read its length or angle. No
command, no undo step: the tool changes nothing.

## Read first

- `docs/interaction-feel.md` — row **CAD**.
- `docs/briefs/S-2-readouts-and-xy-plots.md` — area/inertia readouts (reuse, do not duplicate).
- `crates/gradiance-interaction/src/tools/context.rs:345` — `DraftTool`.
- VectorCraft `crates/tools/src/addons/measure_inspect.rs`, `crates/geom/src/curvature.rs` at `9fff48d`.

## Facts

- Snapped ends come from C-1.
- Area exists after S-2 as a facade read; perimeter and curvature are new
  pure functions over `ShapeContours` (curvature from three neighbouring
  contour points; exact for circle and capsule leaves).

## Approach

- Feel: **CAD**. A `ManipTool` (it reads, never commits) with a readout box
  next to the pointer through the overlay, values in the display unit.
- `geometry::measure` (pure): `perimeter`, `curvature_at`.
- A `measure.between` query operation returning the same numbers for Lisp,
  remote and MCP.

## Out of scope

Persistent dimensions (C-5).

## Done when

- Pure tests: circle curvature radius exact; box perimeter; polygon area.
- Behaviour test: drag between two body centres reads their distance.
- Gates green; screenshot on the PR.

## Outcome
