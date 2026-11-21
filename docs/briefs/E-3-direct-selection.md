# Direct selection: anchors, handles and polygon vertices
Phase E · roadmap § "Unified plan" · status: ready (after E-2)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 9). Docs only.

## Goal

A Direct Selection tool edits a body's outline after it is made: click an
anchor (or a polygon vertex) to select it, `Shift`-click to add, marquee to
select many; drag to move the selected anchors (snapping through C-1); drag
a handle to reshape a curve (the opposite handle stays collinear on a smooth
anchor; a modifier breaks it); click a segment to select its two ends. One
drag is one `reshape` command. A box or circle becomes a path on its first
reshape (one step, its outline unchanged).

## Read first

- `docs/interaction-feel.md` — row **Illustrator**; invariants 1-2 (one gesture, one command; previews only during the drag).
- `crates/gradiance-interaction/src/tools/select.rs`, `handles.rs` — selection and handle hit tests.
- `crates/gradiance-interaction/src/selection.rs` — the `SelectTransition` FSM (anchor selection is tool-local state, not body selection).
- VectorCraft `crates/tools/src/direct.rs` at `9fff48d`.

## Facts

- No vertex editing exists for any shape today.
- `Csg` trees: an anchor edit applies to a leaf that is a direct child of
  the body (a plain `Polygon` or `Path`, optionally under one `Placed`).
  Deeper trees are refused with a status-bar reason (bake first is a later
  option).
- Joints, nodes and dimensions anchor by body-local point; moving the
  outline does not move them.

## Approach

- Feel: **Illustrator**. A `ReshapeIntent { body, shape: ShapeDef }` →
  `ReshapeCommand` through the `command_intents!` table; the tool previews
  on a transient copy (gizmo outline), never the authored shape, until
  release.
- Centroid re-centring: after a reshape the body's origin may move off the
  centroid (allowed, as CSG reshapes do); do not re-centre silently.
- `body.reshape` operation taking an anchor list (indexed target).
- Box/Circle → Path conversion is pure (`geometry::path::from_shape`).

## Out of scope

Adding/removing anchors (E-4). Editing inside CSG trees.

## Done when

- Behaviour tests: moving one vertex of a polygon is one undo step and
  undo restores the exact outline; dragging a handle of a smooth anchor
  keeps the opposite handle collinear; a box reshaped once is a path with
  the same area.
- Gates green; screenshot on the PR.

## Outcome
