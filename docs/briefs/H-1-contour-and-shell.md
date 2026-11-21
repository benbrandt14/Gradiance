# Contour (offset) and hollow shells
Phase H (shape finishing) · roadmap § "Unified plan" · status: ready (after Phase E)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 14). Docs only.

## Goal

A Contour tool: drag away from a selected body's edge to grow it, toward the
inside to shrink it; a floating bar above the selection holds the distance
(a typed field), the corner style (round / sharp) and **Hollow**, which
turns the body into a shell of that thickness — buckets, pipes, cups and
containers from any shape in one gesture. One drag or one bar edit is one
command.

## Read first

- `docs/interaction-feel.md` — row **Illustrator** (on-canvas drag, floating bar); invariant 3 (typed values).
- `crates/gradiance-geometry/src/shape.rs` — `ShapeDef`, `CsgOp`; `sdf.rs` eval/aabb.
- VectorCraft `crates/tools/src/addons/contour.rs`, `crates/ui-egui/src/canvas/contour_bar.rs` at `9fff48d`.

## Facts

- On an SDF, a round offset is exact: `d - r`; a shell is `|d| - t/2`.
  A sharp (mitred) offset is not a field operation; it exists only for
  `Box`/`Polygon`/`Path` leaves by offsetting their outline (kurbo or
  polygon offset) at edit time.
- A shell of an open-top container needs a cut: Hollow then the existing
  cut tool (no new cut logic).

## Approach

- Feel: **Illustrator**. New tree nodes `ShapeDef::Offset { by, shape }`
  and `ShapeDef::Shell { thickness, shape }` (aabb grows by `by` / `t/2`;
  `validate` keeps the result non-empty). Live: the inner tree stays
  editable (E-3 edits through one `Offset`/`Shell` like through `Placed`).
- Sharp corners: a pure `geometry::offset::offset_outline` for leaves,
  baked into a new leaf.
- Bar: a small egui area anchored above the selection, drawn from a
  published mirror; it invokes `body.offset` / `body.shell` operations.
- `FORMAT_VERSION` bump.

## Out of scope

Variable-thickness shells. The contextual task bar (B-2) — this bar is
tool-local.

## Done when

- Pure tests: offset of a circle is a circle of `r + by`; shell area of a
  box equals outer minus inner (± 0.5 %).
- Behaviour test: a hollowed, cut-open box holds water (N-7) — a floating
  ball stays inside it while it sinks with load.
- Gates green; screenshot of the bar.

## Outcome
