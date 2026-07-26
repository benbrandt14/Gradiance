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
| Escapes a bad local minimum | no | yes (annealing) |
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
objective.rs  Metrics: extent + overlap + boundary → one comparable scalar
solver.rs     the Solver trait and PackRun (convergence, best-so-far, restarts)
solvers/      shelf.rs, relax.rs, anneal.rs
rng.rs        splitmix64 — reproducibility without a dependency
```

`PackRun` owns everything *around* an iteration: the stopping rule, the
best-so-far, restarts from new seeds. A solver only advances its own layout
by one step. That is why adding a fourth strategy is one file plus one row
in `solvers::build`, with no stopping logic to re-derive.

## The three solvers

They are genuinely different search strategies, not tuning presets — an
instance one handles badly is usually easy for another.

- **Shelf** (constructive, one-shot). Sort largest-first, lay into rows,
  first fit wins. Instant, deterministic, and it *discards* the current
  arrangement — the right first press on a scattered pile. Reasons about
  bounding boxes, so its results are always legal but never as tight as
  relaxation's.
- **Relaxation** (default). Alternates a Jacobi separation sweep with a
  compaction pulse that squeezes everything toward the centre. Preserves the
  arrangement the user already has and animates legibly.
- **Annealing**. Metropolis over nudges, turns, and *position swaps*. The
  only one that can escape a bad local minimum; the swap move is most of why
  it beats relaxation when it does.

### Two non-obvious things that had to be got right

**Compaction must pulse, not pull continuously.** The obvious relaxation —
apply separation and attraction on every iteration — does not converge to a
legal layout. The two forces reach a *force balance*, leaving a permanent
residual overlap proportional to the attraction gain, and the run then
reports convergence on an arrangement whose bodies are inside each other.
Alternating instead (squeeze, then separate until clear) means every squeeze
starts from a legal state and the best-so-far is always taken from a settled
moment. The gate is on measured overlap rather than an iteration count,
because how long settling takes depends entirely on how tangled the input
was — a fixed period re-squeezes a deep tangle before it has come apart and
never reaches a legal state at all.

**Annealing needs a biased proposal distribution.** A symmetric random nudge
almost always *grows* the bounding box (any vertical component widens a flat
row), so plain Metropolis spends its whole budget rejecting and barely
improves. Leaning proposals inward makes shrinking moves the common case and
lets the acceptance test do the thing it is good at: deciding which of them
survive the overlaps.

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
  edge length) over the same relaxation core.
- **Constraint-driven assembly** — "these two edges touch", "this stays
  above that" as additional penalty terms in `objective.rs`.
- **Scripted optimization** — `PackConfig` already derives `Reflect` and is
  addressable by the operation registry; a `(pack! ...)` verb is a registry
  row plus a `StartPackRequest`, not a new mutation path.
