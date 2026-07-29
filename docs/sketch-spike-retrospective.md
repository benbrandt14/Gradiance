# Retrospective: the constrained-sketching spike

Written at the point where the spike has to be either merged or restarted, so
it is written to inform that decision rather than to defend the work.

## What it was supposed to be, and what it became

The spike was scoped as **a separate sketch mode that does not replace existing
tools** — explicitly throwaway: *"if the spike doesn't pan out we throw away the
commits."* It ends as a rewrite of the entire shape-authoring path: the direct
box, circle and polygon tools are deleted, `EditorMode` is gone, and every
authored shape now originates as a sketch document.

Each step was requested. But the cumulative effect is that **the spike is no
longer discardable**, which was the property that justified moving fast in the
first place. That drift is the single most important thing in this document, and
it should have been flagged at stage 2 rather than after it. The moment the
direct tools were deleted, "throw away the commits" stopped being an option and
nobody said so out loud.

Current footprint against `main`: **99 files, +22,432 / −339**. Of that,
~12,100 lines are vendored C++ and ~7,600 are ours (sketch crate 3,656,
slvs-sys 1,422, session/panel/annotations 2,507).

## Was SolveSpace too heavy? Measured, not guessed

| | count |
|---|---|
| Vendored C++ lines | 12,071 |
| Plus a system Eigen dependency | header-only, but a build prerequisite on 3 platforms |
| Constraint types SolveSpace exposes | 38 |
| Types `solve.rs` maps | 18 |
| Types `edit::applicable` will ever offer a user | 16 |
| Types load-bearing for the flows that actually work | ~10 |

The ten that matter are coincident, distance, horizontal, vertical, parallel,
perpendicular, equal-length, point-on-line, midpoint, diameter. That is the
classic 2D set, and their residuals and Jacobians are a few lines each. A
hand-rolled Gauss-Newton solver over those would plausibly be 400–600 lines with
no vendored code, no Eigen, and no `unsafe`.

**What that estimate would not buy**, and what SolveSpace genuinely provides:

- **Degrees-of-freedom counting.** The "fully constrained" readout is the number
  CAD users steer by, and getting it right means rank analysis of the Jacobian,
  not just counting equations.
- **Redundancy and inconsistency detection, per constraint.** "These two
  constraints contradict each other, and here is which" is what makes an
  over-constrained sketch recoverable rather than mysteriously frozen.
- **Robust convergence** on badly-scaled and near-singular systems, which is
  most hand-drawn geometry.

So the honest verdict is conditional, not absolute:

- If the DOF readout and per-constraint failure attribution are load-bearing
  product features, SolveSpace is earning its 12k lines.
- If the realistic use is "draw a box, occasionally dimension an edge", it is
  not, and a small solver would serve better with a fraction of the surface.

Nothing observed in use so far settles that. What is clear is that **the ratio
is bad today**: 38 exposed, 16 reachable, ~10 exercised.

## The layering observation is correct, and there is evidence for it

The suggestion that CAD should have been *a layer over shape definitions*, with
strong interlinking underneath, rather than a replacement for them, is the
sharpest critique available — and the strongest evidence for it is a thing built
during this spike.

Stage 0a exists because sketch-lowering produced `ShapeDef::Polygon` for
everything, which would have turned every circle into a 48-gon. The fix was a
**recogniser** that reads solved geometry and reconstructs the analytic
primitive: one circle entity → `ShapeDef::Circle`, an axis-aligned four-loop →
`ShapeDef::Box`.

That recogniser is a **compatibility shim** in the current design — a
translator that exists to make sketch-authored geometry indistinguishable from
directly-authored geometry. In a layered design, exactly the same code would be
**the contract**: `ShapeDef` stays primary, a sketch is an optional attachment
that can regenerate one, and the recogniser is how the two representations
agree. Same code, honest role instead of an apology.

The layered design also fixes what is currently worst about the result:

- Direct authoring would stay direct. Right now every box drag runs a solve.
- The sketch layer would be genuinely optional, and therefore genuinely
  discardable — the property the spike was supposed to keep.
- "Base tools don't work fluidly" is a direct consequence of routing them
  through a solver they never needed.

## What is actually wrong with the current result

Listed plainly, whether or not it gets merged.

1. **Auto-commit is a hidden mode.** A gesture that starts on an empty sketch
   commits immediately; one that starts on a non-empty sketch does not. That is
   the right *behaviour* and it preserves the sandbox flow, but it is implicit
   state with no indicator. The author cannot see which mode they are in.
2. **Composing a multi-loop profile is undiscoverable.** It requires committing
   one loop, clicking the body to re-open it, then drawing the second. Nothing
   suggests this.
3. **Two snapping systems still exist.** `resolve_cursor` reconciles them at the
   point of use; it did not unify them. The distinction (a sketch hit can become
   a constraint; a body hit only lends a coordinate) is real, but the seam is a
   comparison rather than a design.
4. **`CameraScale` defaults to 1.0** for a world-per-pixel value — a hundred
   times too coarse. Changing it breaks a dozen tests written at the pixel-era
   scale, so it stays wrong and documented. Any code reading it before the
   camera system runs sees a 10-metre snap radius.
5. **The panel appears and disappears based on session emptiness.** Presence as
   UI state is subtle, and it means the constraint list vanishes the instant a
   shape auto-commits — the moment you most want to look at it.
6. **Mixed profile-plus-links sketches are unimplemented** and unaddressed:
   committing them needs more than one command, and the one-gesture-one-command
   invariant has no answer for it yet.

## What was removed in this pass

Everything that could not be reached by a user:

- **Cubic beziers** — the document, solver bridge, lowering, hit-testing and
  tangency all supported them; no tool ever created one. ~200 lines across six
  files, plus two constraint variants (`CubicLineTangent`, and the bezier half
  of `CurveCurveTangent`).
- **`LengthRatio`, `LengthDifference`, `EqualAngle`** — mapped through the
  solver, never offered by `applicable`.
- **`ops::make_centerline`** — implemented and tested, never called.
- Duplicate constraint naming: `annotate` now owns both the canvas token and the
  prose label, where the session had a second exhaustive match.

## What is worth keeping regardless of the decision

These stand on their own and should survive a restart:

- **`JointKind::Fixed`** — a real rigid link, independently useful, and the
  thing `ToolState::Weld` (which merges) could never provide.
- **Analytic recognition in lowering** — needed by *any* design in which
  sketches produce shapes, and more honest in a layered one.
- **`SketchPoint::anchor`** — referencing a body by an opaque `StableId` the
  sketch crate never dereferences, which let a sketch name world geometry
  without giving `sketch` a physics edge.
- **Four bugs found in pre-existing code**: `draw_draft_preview` hardcoding
  `cam_scale: 1.0` (100× oversized markers), the world snap moving the cursor
  before sketch picking saw it, `CLOSE_RADIUS` absolute where the snap radius
  was screen-relative, and a signal-despawn panic fixed early on.
- **The vendoring approach itself**, if SolveSpace stays: pristine upstream at a
  pinned tag, adaptation on our side of the line, enforced by a test.

## Recommendation

**Restart from a layered design, salvaging the list above.**

The code is sound and tested; the *shape* is wrong relative to what is now
wanted. Un-picking stage 2 — restoring direct authoring as primary while keeping
the sketch layer optional — is close to the same amount of work as rebuilding
from the layered premise, and rebuilding gets a coherent result instead of a
reverted one.

The counter-argument is real and worth weighing: nothing here is broken, the
gate is green, and merging costs nothing today. If the sketch layer is likely to
be revisited soon anyway, merging and revamping keeps the salvage list in-tree
rather than in a branch that has to be re-read.

The decision hinges on one question this spike could not answer: **is
constraint-based editing a core interaction or an occasional one?** If core, the
solver is justified and the layering can be fixed incrementally. If occasional,
both the solver and the replacement of direct tools are over-built, and a
restart with `ShapeDef` primary is the cheaper path to something fluid.
