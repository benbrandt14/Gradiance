# Filleted merge
Phase 4 · M21 · "Smooth-union (fillet) modeling tools (12)" · status: done
(after 4-1; it reuses 4-1's boolean command)

## Goal

The user can join bodies **with a fillet**: a smooth blend of a chosen radius
where they meet, instead of a sharp crease. `CsgOp::SmoothUnion { radius }`
already exists in the geometry layer, with field evaluation and bounds, but
nothing authors it.

This brief makes it a modelling operation that is:
- reachable as an intent;
- reachable as a verb;
- reachable as a context-menu entry with a radius;
- one undo step.

## Read first

- Brief 4-1's Outcome (the boolean intent/command it landed). Fillet is one
  more op through that path, not a new command.
- `crates/gradiance-geometry/src/shape.rs` (`CsgOp::SmoothUnion`) and
  `sdf.rs` ≈L99 (field: polynomial smooth-min) and ≈L186 (bounds inflated by
  the radius).
- `crates/gradiance-geometry/src/polygonize.rs`: the single discretisation
  point. Colliders, meshes and snapping all read the blended contour from it.
- `docs/sdf-geometry-decision.md` ≈L124 (fillets as a reason for the SDF
  model).
- `docs/ui-design.md`: an edit-seam numeric field uses
  `widgets::precise_drag`.

## Facts (orchestrator, 2026-11-14, main `294a47d`)

- No code outside `gradiance-geometry` constructs `SmoothUnion` today.
  *Unverified:* whether polygonize resolves the blend well at small radii.
  Check the contour sample spacing against the radius, and say what minimum
  radius renders cleanly.
- A smooth union can **connect** bodies that a plain union leaves apart,
  when they are within the radius. Merge's "must be one connected
  component" rule then passes. That is correct: it is what a fillet does.
- The fillet radius has to come from somewhere at gesture time. Options:
  - a `ToolDefaults` field (config seam);
  - a value in the menu entry;
  - a verb argument.
  The verb takes it explicitly. Choose the menu's source; the thinnest one
  that works (`docs/orchestration.md` § Future intent 1: invest in the op
  surface, not the chrome).

- **4-1 landed (#232):** `BooleanIntent { op: CsgOp, targets }` →
  `MergeCommand::with_op`; `CsgOp` derives `Reflect`. A `SmoothUnion` already
  flows through it mechanically but is labelled `merge` in undo, and union
  ops refuse disconnected results. 4-1's Outcome has the operand and joint
  rules; fillet inherits them.

## Approach

1. Add the fillet op to 4-1's boolean intent, with radius > 0 validated:
   refuse non-positive or NaN, as a visible error from the verb.
2. Add a verb, e.g. `(fillet-merge a b radius)` (name it to match 4-1's
   family), and an `OpSpec` row.
3. Add a context-menu entry for a multi-selection, with a radius source.
4. If the screenshot shows a polygonize artefact at the blend, its fix is in
   scope **only** where it lives in polygonize's sampling of this op.
   Otherwise record it as a follow-up.

## Out of scope

Variable-radius fillets, chamfers, filleting existing corners of a single
body, smooth subtract/intersect, a dedicated fillet tool in the palette,
`docs/state.md`.

## Done when

- Integration tests:
  - two overlapping boxes fillet-merge to one body whose field matches
    `SmoothUnion` at sample points;
  - two bodies a gap < radius apart now merge;
  - a bad radius errors and commits nothing;
  - one undo restores both.
- Screenshot via `tools/screenshot/`: two boxes in an L joined with a
  visible fillet, next to a plain merge for comparison. Describe it in the
  PR, keep it out of git, and report the path.
- `docs/scripting.md`, and the roadmap M21 fillet line marked landed. Gates
  green. Outcome filled.

## Outcome

**Status: done.** A filleted merge is one more op through 4-1's path:
`BooleanIntent { op: CsgOp::SmoothUnion { radius }, targets }` →
`MergeCommand::with_op`. There is no new intent row and no new command.

**Validation.** `CsgOp::validate()` is new in geometry and is shared by
`ShapeDef::validate`. A smooth union's radius must be finite (`NonFinite`)
and at least `MIN_SHAPE_SIZE` (`Degenerate`). `MergeCommand::stage` calls it
first, so any surface (menu, verb, remote, a raw intent) that emits a bad
radius is refused with `CommandError::Shape` and records nothing. The verb
also checks the radius itself, as a visible script error that names the
verb. The run is all-or-nothing, so nothing else that the run queued
commits.

**Names.**
- Verb: `(fillet-merge a b radius)`, indexed like `merge`. A bad index, or a
  body against itself, is the quiet no-op that the other boolean verbs use.
  It is not `fillet`, because filleting the corners of a single body is out
  of scope and would want that name.
- Undo label: `fillet-merge`. It is one constant, `intent::name::FILLET_MERGE`,
  which the registry re-exports as `CUT` is, so the verb, the label and the
  command are found by one grep. Plain union is still labelled `merge`.
- `OpSpec` has 3 args, in `boolean_specs()`. There is an `EditBinding` row.

**Menu radius source: a value in the menu entry.** "Merge with fillet" sits
directly under "Merge into one body". The row under it is "Fillet radius
(m)" (default 0.25, range 0.01 to 10). Its value is held on the
`ContextMenu` resource (`FilletRadius`), kept between openings, never saved,
and never written to the world. The field is `widgets::labelled_drag`, not
`precise_drag`: it is a parameter of the next gesture, not authored state.
The button commits exactly one `BooleanIntent`, so the field has no undo to
collapse. This is thinner than a `ToolDefaults` field, which would need a
serde default, a scene-content/workstation classification, and a settings
UI (orchestration § Future intent 1).

**Verified facts.**
- *Gap bridging is `gap < radius / 2`, not `gap < radius`.* At the
  midpoint of a gap g between parallel faces, both fields are g/2. The
  quadratic smooth-min lowers that by at most radius/4, so the blend closes
  exactly when g < r/2. Measured: unit boxes with r = 1 join at gap 0.49
  and stay apart at 0.51. Pinned in a geometry test and an integration test.
- *Fillet geometry.* The blend reaches about `radius` along each face from
  a concave corner. On the bisector it sits at 0.25r, where a circular
  fillet sits at 0.29r, so "radius" is close to the circular-fillet radius.
- *Minimum clean radius: about 4 contour cells.* Here a cell is the shape's
  extent / `FIELD_CELLS` (96), so about 0.1 m on a 2 m body. Contour
  vertices are bisection-refined onto the true blend, so the chord error is
  about 1 to 2 mm at every radius. What changes with the radius is how many
  vertices the arc gets. Blend vertices on the 2.4 m test L:
  - r = 0.05 m (2.3 cells): 3, which reads as a chamfer;
  - r = 0.1 m (4.4 cells): 6;
  - r = 0.3 m (11 cells): 9.
  
  Pinned by `a_fillet_of_four_cells_resolves_into_a_curve`. At 100 px/m,
  any radius below 4 cells is under ~10 px anyway. I did not change the
  sampling (the screenshot shows no sampling artefact).

**Screenshot finding: flush faces bulge (not a polygonize artefact).**
Where the two operands share a coplanar outer face (the post sits flush on
the bar's end, both left faces at x = 0), the smooth-min also blends
*along* that face. It bulges outward by up to r/4 at the seam: a 7.5 cm
"knee" at r = 0.3. This is the field of `SmoothUnion` itself; the contour
traces it faithfully. So by the brief it is out of scope and recorded as a
follow-up. Fixing it is a modelling choice: for example a circular or
exponential smooth-min, or blending only where the surfaces meet at an
angle.

**Tests.**
- `tests/it/booleans.rs`:
  - `a_fillet_merge_of_two_overlapping_boxes_is_their_smooth_union`: the
    host field equals the `SmoothUnion` tree at 475 sample points (1e-5);
    the concave corner is filled where a plain union is empty; the collider
    builds; one undo step labelled `fillet-merge`; undo is record-equal;
    redo works.
  - `a_fillet_merge_joins_bodies_a_gap_apart`: at gap 0.3 with r = 1, plain
    merge is refused and fillet-merge joins into one component. At gap 0.7
    (< r, > r/2) the fillet merge is refused.
  - `a_bad_fillet_radius_commits_nothing`: 0, -0.25, NaN and inf give no
    history and a record-equal scene.
  - The label table gains `fillet-merge`.
- `tests/it/scripting.rs` `fillet_merge_blends_two_bodies_and_rejects_a_bad_radius`:
  0, negative, NaN and a string are visible errors naming the verb, and a
  spawn queued in the same run never lands. A good radius gives one
  `SmoothUnion` body, and one undo restores both.
- Geometry: `CsgOp::validate`, blend resolution, and the gap rule.

**Screenshots** (not in git):
- `4-2-before.png`: three touching L pairs.
- `4-2-fillet-vs-merge.png`: the left pair plain-merged (sharp inner
  corner, no seam) and the middle pair `(fillet-merge … 0.3)` (smooth
  concave fillet, plus the flush-face knee above).
- `4-2-menu.png`: the multi-selection menu with "Merge with fillet" and
  "Fillet radius (m) 0.250" under Merge.
- `4-2-after-menu.png`: the right pair fillet-merged from the menu.

**Follow-ups (not done):**
- The flush-face knee (above).
- The selection box of a fillet-merged body ignores the knee (handles at
  the boxes' extent, while the bulge reaches ~7 px past it). It is probably
  the selection bounds reading the leaf extents rather than the contour.
  Not investigated.
- Smooth subtract/intersect, chamfers, and filleting one body's corners
  stay out of scope (brief).
- The 4-1 follow-up "SmoothUnion labelled merge" is resolved.
