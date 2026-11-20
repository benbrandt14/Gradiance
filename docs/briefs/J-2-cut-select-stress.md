# Cut, selection and boolean stress test
Audit · owner request 2026-11-20 · status: in review

The owner's request, verbatim:

> In a new thread, repeat the stress test to resolve issues with cutting,
> selection filtering and boolean operations. There are instances where
> certain objects ignore a cut, or cannot be click selected. Usually after
> something has moved and they seem somehow to be related. There are also
> cases where the cut is not correctly applied. Stress test these
> interactions to find bugs, and resolve them.

Written by the worker, 2026-11-20, in the audit slot after J-1. Same
method as J-1 (`docs/briefs/J-1-joint-stress.md`).

## Goal

Every cut, click selection and boolean, before and after moves, cuts,
undo and play, does what it shows: a stroke across a body cuts it, a
click picks the body drawn under the cursor, a moved body keeps its
authored state. Each defect gets a test that fails first, and a fix
through the existing seams.

## Method

`tests/it/cut_select_stress.rs`: scenes are a few lines of Lisp, or tool
gestures with an injected cursor at the app's default zoom (1 m = 100 px,
so the cut stroke is 3 cm). Live checks ran the real app on lavapipe
(`tools/screenshot/`); images are in the project files under
`cut-select-stress/` (not in git).

## Findings

| # | Area | What | Status |
|---|---|---|---|
| 1 | Cut | A cut was ignored on a long body at some positions: a 4 m box ignored 5 of 20 strokes 2.3 mm apart, an 8 m bar 7 of 20. The 3 cm slit is thinner than the contour cell (body extent / 96: 4.2 cm for 4 m), so it fell between two sample rows; where it fell depends on where the body sits, so a body that moved could stop being cuttable. | **fixed**: the cut contours in the stroke frame on a lattice with a sample row on the stroke line (`polygonize_components_on_axes`; tests `a_thin_cut_severs_a_long_body_wherever_it_crosses`, `a_tilted_thin_cut_severs_a_long_rotated_body`) |
| 2 | Cut | A stroke through one side of a ring did nothing: the rule required two pieces. | **fixed**: a stroke that passes through the material applies with one piece; a stroke that ends inside still does nothing (tests `a_cut_through_one_side_of_a_ring_opens_it`, `a_stroke_that_ends_inside_a_body_still_does_nothing`) |
| 3 | Move | A move with the select tool recorded the body as `Kinematic` in its undo step (the drag holds it kinematic). Undo then redo left the body frozen in the air. | **fixed**: the hold keeps the authored kind in `HeldKind`; `BodyPhysics::capture` reads it (test `a_moved_body_stays_dynamic_through_undo_and_redo`) |
| 4 | Click | Two bodies on one depth band that overlap (one moved onto another while paused): the GPU drew either one on top, while a click picked by entity index, which a cut, a delete or an undo reshuffles. The body that looked on top was not clickable where they overlap; the click selected the hidden one. | **fixed**: newest on top. `StableId::new` mints time-ordered ids (UUID v7); the hit-test breaks a band tie by the newest id; `render::stack` sinks older bodies of a band by 4 mm steps so the newest draws in front (test `a_click_on_two_overlapping_bodies_of_one_band_picks_the_newest`) |
| 5 | Click | Click selection after a move (paused and playing), after play, after a cut (pieces), after merge and subtract, in the select tool and click-through from other tools. | no finding |
| 6 | Boolean | Merge, subtract (split), then a cut through the merged body after a move. | no finding |

## Outcome

**In review (2026-11-20, worker).**

### Landed

1. `cut_cmd::CutCommand::pieces`: the subtract tree in the stroke frame,
   contoured by `polygonize::polygonize_components_on_axes` (the single
   discretization point keeps its role); `contour::contour_components_on_axes`
   samples on a lattice through the origin. Same sample count as before.
2. `CutCommand::passes_through`: one piece applies when both stroke ends
   are outside the material and the stroke crosses it.
3. `domain::props::HeldKind` + `physics::hold` + `BodyPhysics::capture`.
4. `StableId::new` is UUID v7 (no new crate; the `v7` feature of `uuid`).
   Script indices are now creation order for new bodies.
5. `tools::HitQuery` / `bodies_at_sorted` (band, then newest id);
   `render::stack::stack_bodies` (derived `Transform::translation.z`,
   written only on a spawn, despawn or band change); body outlines follow it.
6. `tests/it/cut_select_stress.rs`: 7 scenes; 5 failed on the code before
   the fixes (`a_move_while_playing_records_the_authored_kind` and
   `a_stroke_that_ends_inside_a_body_still_does_nothing` are guards).

### Decisions

Three `decision: true` entries in `docs/review/archive.ron`:
`cut-thin-slit`, `cut-through-ring`, `stacking-order`. No interactive
question was needed.

### Not checked

- `cargo test --workspace` (CI).
- Piece corners keep the contour accuracy (a cut box's corners are
  chamfered by under one cell, about 1 cm on a 1 m box). Exact corners
  need polygon clipping (decision `cut-thin-slit`, option 2).
- A band with more than 32 overlapping bodies: the older ones share the
  deepest step and fall back to the GPU's order.
- Bodies saved before this change keep v4 ids: their order among
  themselves is fixed but not their creation order.
