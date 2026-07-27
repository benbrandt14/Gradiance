# Array repetition — the alt-drag tool

**Status:** accepted, landed.

## What it is

Hold `Alt` and drag one of the selection's scale handles: the selection
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

`Alt` was the free modifier in the select tool: `Ctrl` is duplicate-drag,
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
