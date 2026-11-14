# Filleted merge
Phase 4 · M21 · "Smooth-union (fillet) modeling tools (12)" · status: ready
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

*(worker fills in)*
