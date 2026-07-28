# Layout optimization — `gradiance-optimize`

**Status:** accepted, first slice landed (close packing of a selection).

## What this is

A crate for *arrangement problems*: given a set of shapes and a rulebook,
decide where to put them. The first and so far only problem class is **close
packing** — rearrange a selection to fit into the smallest area without
collisions — but the layer is deliberately shaped as a family rather than as
one algorithm, because the same machinery (a problem description, an
objective, a stepped solver, a preview, one commit) is what every future
arrangement feature needs.

The crate is pure math, in the same sense as `gradiance-geometry`: no
systems, no queries, no `World`. Its only Bevy surface is the
`Resource`/`Reflect` derives on `PackConfig`. The ECS half lives in
`interaction::pack`.

## Why not just use the physics engine

This is the load-bearing decision, so it is worth stating plainly. The
obvious way to squash a pile of bodies together is to crank gravity, or add
an attractor, and let avian settle them. That is a fundamentally different
thing and a worse one for this job:

| | physics settling | this crate |
|---|---|---|
| What it computes | a *simulation* of an arrangement forming | a *search* over poses |
| What it optimizes | nothing — there is no objective | an explicit scalar you choose |
| "Smallest" | whatever the dynamics fall into | bounding area, hull area, enclosing circle, hull perimeter |
| Extra inputs | mass, density, restitution, friction, sleeping, time step | none |
| Reproducible | no (solver order, substep timing) | yes, from a seed |
| Escapes a bad local minimum | no | yes (warm start + restarts, if ever needed) |
| Tunable goal | no | parallel edges, contact, density, aspect … |
| Container / aspect targets | no | yes, hard or soft |
| Cost of a "what if" | re-simulate | re-solve, interruptibly |

The user asked for the arrangement with the smallest area. An objective
function is the only way to *ask* that question, let alone answer it. Once
there is an objective, the natural implementation is a geometric search —
and once it is a geometric search, none of the physics inputs mean anything
and all of them are in the way.

There is a second, practical reason: a physics settle cannot be previewed
without actually moving the bodies, so it cannot be cancelled cleanly, and
it cannot be one undo step. A search over poses is a pure value until the
moment it is committed.

## Structure

```text
problem.rs    PackItem / PackProblem / PackConfig — what is being arranged, under which rules
hull.rs       convex hull, area, centroid, placement
sat.rs        separating-axis overlap → minimum translation vector
objective.rs  Metrics: the weighted terms → one comparable scalar
gradient.rs   PackEnergy: layout ↔ parameter vector, cost and analytic gradient
solver.rs     the Solver trait and PackRun (convergence, best-so-far, restarts)
solvers/      shelf.rs, descent.rs, naive.rs
rng.rs        splitmix64 — reproducibility without a dependency
```

`PackRun` owns everything *around* an iteration: the stopping rule, the
best-so-far, restarts from new seeds. A solver only advances its own layout
by one step. That is why adding a fourth strategy is one file plus one row
in `solvers::build`, with no stopping logic to re-derive.

## The objective is a weighted sum, not one number

`ObjectiveWeights` combines several **dimensionless** terms, which is what
lets the same machinery express "pack this as tight as possible", "line these
up", and "spread these out inside a box" without a different solver for each:

| term | what it rewards |
|---|---|
| `extent` | a smaller overall bounding measure |
| `fill` | filled area ÷ bounding area → 1 |
| `gap` | less leftover space between *neighbours* |
| `parallel` | edges pointing the same way (or axis-aligned) |
| `contact` | signed — less touching, or flush interlock |

Two normalization decisions are load-bearing. Terms are scaled against a
**reference length derived from total item area**, so a weight means the same
thing in a 10 cm scene and a 40 m one — otherwise weights could not be saved
settings. And that reference is **constant for the run**, not the current
extent: normalizing by a quantity that moves as the layout does makes the
objective non-stationary, and gradient methods chase their own tail.

### The gap term, and what benchmarking actually showed

The gap term was added on a plausible theory: extent is a *global* measure,
so a body in the middle of a cluster does not move the bounding box at all
and receives no signal, while a local gap measure would give every body a
direction. It was expected to be the biggest quality win.

**It was not.** Measured across the three reference scenes it is neutral for
density at best, and when it was wired into the iterative solver's
compaction pulse it was actively worse — a local pull forms tight clumps that
each close their own gaps while the arrangement as a whole stays loose. That
wiring was removed and the default weight is **0**. It survives as a *goal*
you can ask for ("hold this spacing between neighbours"), not as a free win.

It also had a genuine design bug worth recording. Scored over "every pair
within radius R", the term has a trivial exploit: spread everything far
enough apart that no pair is within R and the cost reads zero — so exploding
the arrangement is a global minimum, and a strong weight collapsed fill from
0.90 to 0.04. It now measures a fixed **count** of nearest neighbours
(`gap_neighbors`), which can never empty out.
`a_strong_gap_weight_cannot_be_escaped_by_spreading_out` guards it.

## Warm starts removed

The warm start seeded an iterative solver with a constructive shelf packing.
It made the measured numbers much better and the behaviour much worse to
reason about:

- the result depended on a hidden pre-pass, so what you got was not what the
  solver you selected actually did;
- running a pack twice did not do the same thing twice, because the second
  run started from the first run's output;
- and — the bug in the previous revision — a solver that was doing nothing at
  all was indistinguishable from one that was working, because the shelf
  layout it had been handed carried the result.

It is gone: `build` now constructs the solver you asked for and nothing else,
and every solver starts from the poses on screen. `SolverKind::Shelf` is the
default, being the only strategy that reliably produces a tidy packing
unaided. Descent stays, honestly labelled: from a scattered pile it converges
to the scattered pile, and fixing that is the next optimizer spike's job.

`Solver::seed` remains on the trait with a test pinning its contract — the
spike will want to hand descent a starting arrangement, deliberately and
visibly, rather than have one applied behind it.

## The solvers

Genuinely different search strategies, not tuning presets — an instance one
handles badly is usually easy for another.

- **Shelf** (constructive, one-shot). Sort largest-first, lay into rows,
  first fit wins. Instant, deterministic, and it *discards* the current
  arrangement — the right first press on a scattered pile. Reasons about
  bounding boxes, so its results are always legal but never as tight as
  relaxation's.
- **Descent** (default) — L-BFGS from the `argmin` crate, over an analytic
  gradient. See below.
- **Naive** — the baseline. See below.

### Two that the benchmark retired

An earlier revision also shipped a **penalty relaxation** (Jacobi separation
sweeps alternating with compaction pulses) and a **simulated annealer**
(Metropolis over nudges, turns, and position swaps). Both worked. Neither
earned its keep once the numbers were in:

| scene | Shelf | Relax | **Descent** | Anneal |
|---|---|---|---|---|
| uniform | 1.000 | 0.982 | **1.000** | 1.000 |
| mixed | 0.681 | 0.465 | **0.693** | 0.681 |
| bars | 0.898 | 0.610 | **0.898** | 0.898 |

Annealing scored *identically to shelf* on all three scenes — once warm
started from a shelf packing it had nothing left to discover, and the
acceptance test could not improve on what it was handed. Relaxation was
dominated everywhere, by a wide margin on the two hard scenes. So the
"different strategies cover each other's bad instances" argument, which is
the entire justification for carrying more than one, turned out not to hold
for these two: they had no instance they were best at.

Deleting them took ~1,350 lines with their tests, params, tuning panels, and
the `rng` module — and it took the **restart machinery** with them. Restarts
and the run seed only ever bought anything for a stochastic search; with
every remaining solver deterministic, re-running one reproduces the same
layout exactly. Keeping a "restarts" slider that cannot change the answer
would have been worse than not having one.

What survives has three distinct jobs rather than five overlapping ones:
build an arrangement, polish one, be the yardstick. If a future instance
genuinely defeats descent, adding a strategy back is one row in
`solvers::build` — but it should arrive with a scene it wins on.

## Gradient descent: argmin owns the algorithm

`SolverKind::Descent` uses **argmin** (pure Rust, no C toolchain) for the
L-BFGS two-loop recursion, the curvature history, and the Hager–Zhang line
search. This crate supplies only the cost and the gradient. That division is
deliberate: a line search that reliably satisfies the strong Wolfe conditions
is exactly the kind of numerical code that is easy to get subtly and silently
wrong, and none of it is packing-specific.

Two things had to be solved to make it fit:

**Stepping.** argmin's usual entry point (`Executor::run()`) iterates to
completion in one blocking call, which is unusable when the editor is drawing
the search as it converges. The solver instead drives argmin's `Solver` trait
manually — `init` once, then `next_iter` per frame — which keeps the
curvature history alive across frames. Restarting an executor each frame
would discard it and reduce L-BFGS to plain gradient descent.

**The gradient.** Translation is analytic and nearly free: SAT already
computes the separating axis `n`, and `∂(separation)/∂posⱼ = +n`, so the
overlap and gap terms differentiate to a sum of `±n` contributions — the same
quantity SAT already yields as a minimum translation. Rotation gets one forward
difference per item (differentiating through moving witness features is not
worth the machinery), which is why `RotationMode::Fixed` is dramatically
cheaper. The extent term gets a *descent direction* rather than its true
subgradient, because a bounding box's subgradient is supported on a single
extreme vertex and following it moves one body at a time; the line search
only needs a direction that goes downhill and verifies that itself.

Descent strictly descends, so it inherits whatever basin it starts in —
hence `warm_start`, which seeds it from a shelf packing.

## The naive baseline, and why it is in the tree

`SolverKind::Naive` is attraction-to-centroid plus overlap separation, both
on every iteration — what turning up gravity in the physics engine amounts
to. It is the most obvious thing to reach for, and it is here **to be
measured against**.

It is deliberately not a straw man: it gets the same separation quality,
clearance handling, boundary clamping, per-iteration step limit, and
best-so-far tracking as the real solvers (a genuine physics settle would not
even have the last of those). The single difference is that attraction and
separation act together, so it settles at a *force balance* rather than at an
arrangement. The equilibrium gap between two bodies ends up set by the ratio
of two tuning gains instead of by anything about the packing: turn attraction
up and bodies interpenetrate, turn it down and they stop early with visible
slack. No setting produces a tight packing, because tightness was never what
it was computing.

`the_real_solvers_beat_the_naive_baseline_on_density` asserts the real
solvers beat it on fill ratio across three scenes, in CI. A solver family
with no yardstick has no way to know whether it is any good.

It is also excluded from the warm start, in code and with a comment: handing
the baseline a constructive packing to start from would be borrowing the
answer from the solver it is meant to be compared against.

## Measured quality

Fill ratio (body area ÷ bounding area; 1.0 is a perfect tiling) on the three
benchmark scenes, at the tuned defaults:

| scene | Shelf | **Descent** | Naive |
|---|---|---|---|
| uniform (12 equal squares) | 1.000 | **1.000** | 0.311 |
| mixed (14 random sizes) | 0.681 | **0.693** | 0.087 † |
| bars (long bars + squares) | 0.898 | **0.898** | 0.065 |

† infeasible — the baseline left bodies overlapping.

Two things drove those numbers more than any objective tuning:

- **Warm starting** the search solvers from a shelf packing — by far the
  largest single lever. The annealer went from 0.33/0.08/0.25 to
  1.00/0.68/0.90 on that one change alone (and thereby stopped being
  distinguishable from the shelf packing feeding it, which is why it is gone).
  Descent cannot work at all without it on a scattered pile.
- **Making the overlap penalty steep enough** (see `OVERLAP_DOMINANCE`). A
  quadratic penalty has a vanishing gradient at contact, so descent settled a
  hair inside every neighbour — infeasible, rejected wholesale by the run,
  and indistinguishable from the solver doing nothing at all.

`SolverKind::Descent` is the default on this evidence.

### The bug that made all of this look like nothing

Worth recording, because the symptom was indistinguishable from "the solver
works and your scene was already optimal".

`solvers::build` seeds an iterative solver with a constructive shelf packing.
`PackRun::new` scored the *problem's start poses* as best-so-far, and scored
again only after the first step. So the warm start — a complete, feasible,
usually excellent candidate layout — was never scored on its own. Descent
would take it, compact, transiently overlap, be rejected as infeasible by
`is_better`, and the run would finish reporting the untouched input.

On twelve scattered unit squares: shelf produced a 3×4 grid at fill 0.972,
and descent — handed exactly that layout — returned the original scatter at
fill 0.091 across ten "columns" and ten "rows".

The fix is to score the layout the solver *starts from*, not just the one it
was handed. Four lines, and it is the difference between the crate working
and the crate appearing to work. `tests/it/packing.rs` now pins the whole
user-facing scenario: scatter a grid of equal rectangles, pack it, assert
clean rows and columns, no overlaps, and fill above 0.75.

The general lesson is about *where* a candidate can enter the system. Any
layout the run can hold — start pose, warm start, solver iterate — has to
pass through the same scoring gate, or the best answer can be thrown away by
a code path that never looked at it.

### Two non-obvious things that had to be got right

**Separation and attraction must not act together.** Applying both on every
iteration does not converge to a legal layout: the two forces reach a *force
balance*, leaving a residual overlap proportional to the attraction gain, and
the run then reports convergence on an arrangement whose bodies are inside
each other. This is not a tuning problem — the equilibrium gap is set by the
gain ratio rather than by anything about the packing. `NaiveSolver` is that
failure, preserved deliberately as the yardstick, and it is precisely what
"turn up gravity and let physics squash it together" amounts to.

**The overlap penalty must be steep at contact.** A quadratic penalty has a
vanishing gradient exactly where it needs to bite, so descent settled a hair
inside every neighbour — infeasible, rejected wholesale, and indistinguishable
from the solver doing nothing. `OVERLAP_DOMINANCE` is the fix.

## Depth is a first-class constraint

This is a 2.5D packer, not a flat one. Two bodies may share an XY footprint
exactly when their collision-layer bits are **disjoint** — that is, when
they sit at different depths and would never touch in the running scene.
Since `LayerRule::Respect` is the default, packing a selection that spans
several depth bands naturally interleaves it, and the result is much tighter
than a flat packing of the same set. `LayerRule::Solid` turns this off for
when the packing is a *layout* (a parts sheet, a diagram) rather than a
physical arrangement.

This falls straight out of the existing invariant that collision layer ≡
visual depth: the packer derives its layer masks from `DepthBand::bits()`,
the same function the collider sync uses, so "will not collide" means the
same thing in both places by construction.

## Deliberate simplifications

- **Items are convex hulls.** Convex overlap has an exact, cheap answer
  (SAT), which is what lets a solver ask "how deep, and which way out"
  thousands of times a second. The cost is conservatism: a concave body
  packs as its hull, so nothing nests into its own concavity. Results are
  therefore always collision-free in the real scene, never tighter than the
  true optimum. Non-convex nesting would need a convex decomposition per
  item and a no-fit-polygon inner loop; the `Solver` trait is where that
  arrives, as a new item representation rather than a new mutation path.
- **Feasibility has a tolerance** (`Metrics::PENETRATION_TOLERANCE`, 10 µm).
  Layouts are *placed* by one code path and *scored* by another, and the two
  disagree in the last few bits — two perfectly flush boxes score a ~3 µm
  penetration. Testing against exact zero rejects good layouts as illegal,
  which is worse in every way than a tolerance four orders of magnitude
  below the default clearance.

## The UI lives in a window, not the Properties pane

The rulebook started as a collapsing section in the Properties dock pane and
was moved out. Two reasons, one practical and one conceptual: it is far too
tall for a dock pane (it pushed the per-body sections off-screen and needed a
scrollbar inside another scrollbar), and it is not a per-body property — it
describes an operation over the whole selection. A modeless window can also
sit open beside the viewport while the ghost converges, which is exactly how
the settings get tuned.

What stays in the Properties pane is a compact strip: status, Pack /
Apply / Cancel, and a button into the window.

## How it reaches the world

Unchanged command discipline. While a run is live it writes **nothing** —
not authored components, not the command stack, only gizmos. On acceptance
it emits exactly one `CommitTransformIntent`, so the entire rearrangement is
a single undo step regardless of how many bodies moved or how many thousand
iterations it took. `Escape` cancels and leaves the scene untouched.

A pack is not a `ToolState` because it is not a pointer gesture: no press,
no drag, no release, and it has to keep running while the user works in the
inspector. The `PackSession` resource *is* the mode.

Turning a solved layout back into body poses is the one fiddly part. An item
can stand for several bodies (a selection group moves rigidly), and an
item's pose refers to its **hull centroid**, not to any body's origin —
which for a CSG-reshaped body can be far off-centre. So each target records
the body's authored pose and the item pivot it was measured against:

```text
Δrot    = final.rot − start.rot
new_pos = final.pos + R(Δrot) · (body_pos − pivot)
new_rot = body_rot + Δrot
```

## Where this goes next

The crate is named for the general case on purpose. Candidates that fit the
existing seams without new machinery:

- **Nesting** (non-convex, true no-fit-polygon) — a new item representation
  behind the same `Solver` trait.
- **Auto-layout of the node graph** — a different objective (edge crossings,
  edge length) over the same objective core.
- **Constraint-driven assembly** — "these two edges touch", "this stays
  above that" as additional penalty terms in `objective.rs`.
- **Scripted optimization** — `PackConfig` already derives `Reflect` and is
  addressable by the operation registry; a `(pack! ...)` verb is a registry
  row plus a `StartPackRequest`, not a new mutation path.

More objective terms are the cheapest extension of all: each is a function of
the placed hulls plus a weight, and `parallel` and `contact` are worked
examples. Obvious candidates are collinearity of specific edges, a preferred
gap *distribution* rather than a mean, and symmetry about an axis.

Other argmin solvers are nearly free too — `NelderMead` (derivative-free, for
when the surrogate is a poor guide), `ParticleSwarm` (global, and it would
slot in beside descent). Each is one more row in `solvers::build`. The one
caveat is the one this crate already learned the hard way: any gradient-based
addition must be handed a cost and a gradient *of the same function*, or its
line search will hunt forever.
