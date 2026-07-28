# Array repetition — the ctrl-drag tool

**Status:** accepted, landed.

## What it is

Hold `Ctrl` and drag one of the selection's scale handles: the selection
repeats along that axis. Side handles make a row or a column, corner handles
make a grid. By default copies land **flush** — drag a single block sideways
and you get a seamless wall; drag a two-block stack upward and you get a
seamless tower.

## Why it rides the scale handles

An array is a bounding-box operation in exactly the way scaling is: you grab
a side and pull, and the box grows along that side. Reusing the same eight
handles means the affordance is already on screen, already frame-aware
(global or local axes, per the `F` toggle), and already understood — the only
new thing to learn is the modifier. It also makes the two operations read as
siblings, which they are: **scale stretches the content, array repeats it.**

`Ctrl` was the free modifier in the select tool: `Ctrl` is duplicate-drag,
`Shift` is additive selection and uniform scaling.

## The flush pitch is exact, not a bounding box

The interesting part of the feature is deciding how far one step is.

Using the selection's extent along the drag axis is the obvious answer and it
is right only for a convex, axis-aligned selection. Two bodies arranged in a
staircase can *interlock*: stepping by the full bounding width leaves a
visible gap where the copy could have nested. So `geometry::array` computes
the exact answer instead — the smallest `t` for which the translated set no
longer intersects the original.

For convex `A`, `B` and unit direction `d`, `A + t·d` overlaps `B` exactly
when the projections onto every separating axis overlap. Along axis `n`,
translating shifts `A`'s projection by `t·(d·n)`, so the overlap condition is
a plain interval in `t`:

```text
t·(d·n) ∈ (b_min − a_max,  b_max − a_min)
```

Intersecting those intervals over all axes gives the set of `t` for which the
pair overlaps at all. Its supremum is the smallest step that clears the pair,
and the maximum over every ordered pair of pieces is the pitch for the whole
selection.

Two properties make this usable rather than merely correct:

- The overlap set is an **interval**, so clearing at the pitch also clears at
  every larger step. One pitch is therefore valid for copy 2, 3, and 40 —
  not just the first.
- Pairs are taken **ordered and including each piece against itself**, because
  the copy of a piece has to clear every original piece, and its own original
  is usually the binding one.

`interlocking_pieces_step_closer_than_their_bounding_box` pins the difference:
a two-block staircase steps by one block, where a bounding-box implementation
would step by two.

The conservative part: pieces are convex hulls of each body's outline, so a
body with a bite cut out of it steps as though the bite were filled. Results
are always collision-free and never tighter than the true optimum — the same
trade `gradiance-optimize` makes, for the same reason.

## Patterns are data

`ArrayMode::placements` expands any mode into a list of `CopyPlacement`s — one
rigid map plus per-copy tweens — and `ArrayCommand` does nothing but walk that
list. Adding a pattern is one match arm, with no new cloning, joint-remapping,
or group-renumbering logic to get subtly wrong.

It also means a pattern can be *inspected* before it is applied, which is what
lets the tool draw its ghost from the **same list the command will use**. The
preview cannot drift from the result, because it is the result.

| | |
|---|---|
| `Linear` | a row or column |
| `Grid` | two axes, with a `stagger` fraction for running-bond brick walls |
| `Radial` | a sweep about a pivot, optionally turning bodies with it |

Per-copy **tweens** apply to every mode, because they are about the copy's
index rather than where it sits: `spin` (a fan of blades), `scale_ratio` (a
taper), and `depth` — a staircase *through* the 2.5D collision layers, which
also means copies past one layer stop colliding with each other.

Spacing is a separate dial from the pattern: `Contact` (flush), `Gap` (flush
plus a distance), `Fixed` (ignore the geometry), or `Multiple` (a factor of
the flush pitch — 2.0 leaves a body-sized hole, 0.5 interleaves).

## Per-copy change: two lanes, every axis on its own

A pattern that only repeats is a wall. What makes it a *design* is what
changes from one copy to the next — and the shape of that knob is the whole
decision here.

**Lanes, not a counter.** The obvious encoding is one running index `k` and
one tween applied `k` times. It falls apart on grids: cell `(2, 1)` and cell
`(1, 2)` would get the same treatment, so "narrow as you go right, shorten as
you go down" is unsayable. `ArrayTweens` therefore carries **one lane per
pattern axis** — `along_x` indexed by column, `along_y` by row — and a copy
folds in both. A linear array runs one lane; a grid runs both; the lane that
drives a row of copies is the one named after the direction the row runs
(`ArrayMode::Linear::axis_y` records which).

**Sizes are a `Vec2`, inside each lane.** "Which way through the pattern" and
"which way the body stretches" are different questions, and both need
answering: `(0.99, 0.99)` on the column lane is the classic taper, `(0.99,
1.0)` narrows without flattening. Ratios compound (`scale^k`) because "99% of
the last one" is a multiplication; spin and depth accumulate additively. Every
field is inert at its default, so a lane nobody touched costs nothing.

**The frame travels with the tween.** `origin` and `basis` pin the axes the
ratios are measured in to the selection's own centre and rotation at press
time, so "x" in the options panel means the selection's x however it is
turned. The tool writes them; the panel never sees them.

## Keeping contact while the copies shrink

The interesting part. Contact spacing measures one pitch and steps by it —
but if copy `k` is smaller than copy `k−1`, one pitch is wrong for every gap
but the first, and the wall develops a widening seam exactly where the taper
bites.

Re-measuring per copy would work and would be slow (the ghost redraws every
frame of the drag, for up to 512 copies). It is also unnecessary, because the
answer is closed-form. Writing `H` for the selection's outline about its
centre and `u` for the per-copy ratio, copy `k` occupies `u^k ⊙ H`. Clearing
copy `k+1` from copy `k` along a frame axis `d` reduces — by factoring out the
common `u^k`, which is a disjointness-preserving bijection — to

```text
t_k = u_d^k · Q,     Q = contact_pitch_between(u ⊙ H, H, d)
```

So the steps form a **geometric series**: one extra pitch measurement, and
copy `k` sits at `Q · (1 + u_d + … + u_d^{k−1})`. `geometric_span` is that
partial sum, taken at the `u → 1` limit directly so an inert taper is exact
rather than `0/0`.

Three consequences worth stating:

- **It is exact, not an approximation** — but only because handle drags run
  along the frame axes the ratios are expressed in, which is what makes
  `u^{-k} ⊙ d` parallel to `d`. An arbitrary diagonal direction would need the
  per-copy measure.
- **A grid needs the cross terms.** Column pitch inside row `r` carries the
  row lane's *x*-shrink, and row pitch in column `c` carries the column lane's
  *y*-shrink. Without them a doubly-tapered grid is inconsistent — the two
  ways of walking to a cell disagree. `ArrayMode::Grid` therefore takes both
  ratios as `Vec2`s.
- **A converging taper has finite reach.** `0.99^k` sums to a bounded
  distance, so "how many copies fit in this drag" is no longer a division;
  `copies_within` solves the geometric sum, and saturates at the cap instead
  of running away when the drag outruns the limit.

`ArraySpacing::Fixed` opts out on purpose: an explicit step means an explicit
step even while the copies change size. Every other rule tracks the geometry
and therefore tracks the taper.

## The Lisp surface

The user-facing ask was to express the per-copy change in the DSL. The
governing rule (`script-lisp-decision.md`) is that the VM never enters the
per-frame loop — and the ghost re-expands the whole placement list every frame
of a drag, so a Scheme lambda evaluated per copy is exactly the thing that is
forbidden. `gradiance-kernel` exists for cases that genuinely need a compiled
per-element expression; a taper does not, because the closed form above
collapses the whole series to two numbers.

So the tween stays plain reflected data, and the Lisp reaches it the way every
other edit does — through the intent seam, once, on the cold path:

```scheme
(array-repeat b 20 1.0 0 0.99 0.99)   ; 20 copies, each 99% of the last
(define (taper b n r) (array-repeat b n 1.0 0 r r))
```

`array-repeat` is an ordinary Edit verb: it emits one `ArrayIntent` and lands
as one undoable command. If a future pattern really does need an arbitrary
`f(k)`, the place for it is a `kernel::Expr` compiled at press time — not a
VM call per copy.

## The refinement pass

Four changes after using it in anger, each one a thing that was wrong rather
than merely missing.

**`Ctrl`, not `Alt`, and the handles say so.** The modifier moved to the key
people already associate with copying. More importantly the affordance is now
visible *before* the gesture: holding it repaints every handle and gives each
one an outward ghost square, rotated with the frame, so a local-frame
selection shows the offset along its own axes. A modal drag whose mode you
cannot see until you release is a modal drag people mistrust.

**Nearest handle wins.** `hit_handle` took the first handle within the capture
radius. That radius is a fixed number of *pixels*, so on a small or
zoomed-out selection several handles are in range simultaneously and the
answer was whichever led `HandleKind::ALL` — reliably an edge. This is why
grabbing a corner "worked sometimes": it depended on zoom. Nearest now wins,
with corners breaking ties, because a corner is the more specific request —
it asks for both axes, and an edge is always a few pixels away.

**A corner always builds a grid.** Previously a corner drag produced a
`Linear` mode until the second axis had been pulled far enough to earn a row,
then snapped to `Grid`. One pixel of drag changed the kind of thing being
built. It is a `Grid` from the first frame now, with zero rows if you have not
pulled that way yet.

**Fixed counts are per-axis, and invert the gesture.** `count_x` / `count_y`
replace a single `count_override`. With a count fixed the drag stops choosing
*how many* and starts choosing *how far apart* — pull to spread that many
copies over the distance. The pitch floors at the contact pitch, so a fixed
array can be stretched apart but never squeezed into itself.

Two things were also removed. **Radial** is gone until it can carry a
configurable centre of rotation; sweeping about the selection's own centre is
the least useful of the possible pivots and was the wrong default to ship.
**Spacing** is down from four rules to two — contact and contact-plus-gap —
and the gap is clamped non-negative. `Fixed` and `Multiple` both existed to
let a step ignore the geometry, which is the one thing this tool is for; and a
negative gap let the drag ask for a pattern that overlaps itself, which is
never the intent.

## Where the code lives

```text
geometry::array              the flush-pitch math (exact, convex, no ECS)
geometry::hull, ::sat        moved here from gradiance-optimize — general 2D
                             math with two consumers now
interaction::tools::array_tool   drag → ArrayPlan, plus the ArrayConfig rulebook
command::array_cmd           ArrayMode → CopyPlacements → cloned records
ui::array_panel              the options window
```

Moving `hull` and `sat` into `gradiance-geometry` was the architecturally
correct call once a second consumer appeared: CLAUDE.md puts testable math
there, and both are general 2D primitives rather than anything to do with
optimization. No dependency edges changed.

`ArrayConfig` is tool configuration, not scene content — the same carve-out
`PackConfig` and `ToolDefaults` sit in. It is edited directly by the UI
through the Config seam and is deliberately not part of the saved document.

## Command discipline

Unchanged. While the pointer is down the gesture writes nothing but gizmos;
release emits exactly one `ArrayIntent`, so an array of two hundred blocks is
a single undo step. Dragging a handle back through the box produces nothing
rather than copies stacked behind the original — pulling back is how you
cancel. A per-axis cap (`MAX_COPIES_PER_AXIS`) keeps one flick of the wrist
from asking for a hundred thousand bodies when the pitch happens to be tiny.

## A bug this turned up

`without_alt_the_same_drag_scales_instead` — written only to check that the
modifier is the whole difference — failed, and the cause was not the new code.
The scale gesture guarded its factor computation with `start_f.x.abs() > 1.0`,
which meant "at least one pixel from the pivot" before the SI flip. At metre
scale it silently disabled scaling for any selection under a metre across,
which is most of them. It is now a plain divide-by-zero epsilon.

## Where this goes next

- **Mirror** copies, which need shape reflection rather than a rigid map.
- **Path arrays** — repeat along a drawn curve rather than an axis.
- Non-convex nesting, so an `L` can interlock with its own copy. Same seam,
  same cost, as the equivalent extension in `gradiance-optimize`.
- A scripted `(array …)` verb: `ArrayIntent` already carries the whole
  pattern as reflected data, so this is a registry row rather than a new
  mutation path.
