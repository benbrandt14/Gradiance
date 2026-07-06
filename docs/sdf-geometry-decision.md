# Decision: SDF trees as the base geometry representation

Status: **accepted** (2026-07-05). Supersedes the M9 plan of clipper2 polygon
booleans; `clipper2`/`i_overlay` are dropped from the planned dependency set
before ever being added.

## Question

Not "are SDFs easier for the cut tool" (they are not — polygon booleans are
exact and off-the-shelf). The question is: **are there hurdles to making SDFs
the base primitive representation that would block the project's overall
goals** (Algodoo parity, CSG, CAD snapping, joints, persistence, magnetism
fields, 2.5D toon rendering)?

## Verdict

No fatal hurdle. Three permanent constraints must be accepted, and one
architectural shape avoids a heavy hybrid workflow. Adopt now, before more
consumers accrete on `ShapeDef`.

## The three permanent constraints

1. **Physics is polygonal forever.** Verified against the pinned sources:
   avian/parry custom shapes go through `SharedShape::new` with
   `Shape + SupportMap` impls — support maps exist only for *convex* shapes
   (avian's `EllipseColliderShape` works this way), and parry's narrow phase
   handles non-convex geometry only through its own composites (trimesh,
   compound, voxels). A general CSG SDF is neither. Colliders therefore stay
   what they already are: **derived state**, rebuilt by contouring the SDF and
   convex-decomposing the result. Contact fidelity equals contouring
   resolution — controllable, and the same class of approximation the circle
   (48-gon) collider already accepts.

2. **Cut-splitting discretizes regardless of representation.** Separating a
   cut body into connected components is a *topological* operation; min/max
   field booleans cannot express "the left piece". Any representation must
   contour and walk components at cut time. Consequence: multi-piece cut
   results bake to `Polygon` leaves (exactly what the clipper2 plan produced),
   while **single-component results stay analytic** — cutting a notch out of a
   circle is just a `Subtract` node, and the circle's roundness survives
   forever. This also bounds CSG-tree growth: trees deeper than a threshold
   bake to polygon leaves, so repeated cutting cannot degrade performance
   unboundedly.

3. **Contoured boundaries approximate sharp CSG corners** at grid resolution
   (mitigated by Newton-refining vertices onto the zero set via the gradient;
   2D dual contouring is a later upgrade that recovers corners exactly).
   Area-conservation proptests become tolerance-based rather than
   vertex-exact. Related: after min/max the field is a *lower bound* on
   distance, not true distance — fine for contouring and for magnetism
   falloff (M12), only matters for exact offset/shell operations (do those by
   iterative projection if ever needed).

## Rendering

`bevy_smud` is a dead end twice over: its latest release (0.14.0) pins
`bevy ^0.16` (we are on 0.19), and it is a flat screen-space 2D renderer —
it cannot produce the project's signature extruded-2.5D look with cast
shadows. The non-hybrid answer does not need it: the SDF is the **single
authored source of truth**, and one derived `Contours` cache per body feeds
*every* consumer — collider decomposition, extruded `Mesh3d`, snapping
feature points, tessellation. Meshes are a derived *view*, not a second
geometry system; there is exactly one discretization point in the codebase
(`polygonize`). A raymarched extruded-SDF material (fragment-depth +
prepass so shadow maps stay correct) is a strictly render-side upgrade
available later without touching geometry, physics, or tools.

## Architecture (what "SDF as base primitive" concretely means)

- `ShapeDef` *becomes* the SDF tree. Existing variants `Box`, `Circle`,
  `Polygon`, `HalfPlane` remain as **leaves** (their analytic polygonization
  stays exact — no regression for uncut bodies), joined by
  `Op { kind, lhs, rhs }` (`Union`, `Subtract`, `Intersect`,
  `SmoothUnion { k }`) and `Xform { pos, rot, node }` for child placement.
- `geometry/sdf.rs` (pure, proptested): `eval(p) -> f32`, gradient, AABB.
- `geometry/contour.rs` (pure): marching squares over the AABB with
  root-refined vertices → `Contours`. `polygonize()` dispatches: leaves take
  the exact path, ops take the field path. Everything downstream of
  `Contours` — colliders, extrusion, snap sources, persistence, undo,
  inspector leaf editing — is unchanged **by construction**.
- Cut tool: subtract a strip node in body-local space; one component ⇒ keep
  the tree (analytic), several ⇒ polygon-leaf pieces with recentering,
  velocity inheritance `v + ω × r`, joint reattach-or-delete.
- Old scene files parse unchanged (new enum variants are additive for RON).

## Build vs. reuse (ecosystem survey, 2026-07-06)

Surveyed before hand-rolling anything:

- **`fidget` 0.4.3** (actively maintained) is the industrial implicit-
  modeling library — JIT/VM evaluation with interval-arithmetic pruning.
  It earns its weight on *large* expression trees; ours are depth ≤ 8 over
  four leaf kinds, and the authored type must be our own serde enum (it
  *is* the save format), so fidget could only sit behind, not replace, the
  domain type. It is the named **escalation path** if tree evaluation ever
  becomes the bottleneck: swap it in behind `polygonize` without touching
  any consumer.
- **`contour` 0.13** (d3-contour port) does marching squares, but over a
  pre-sampled grid with linear interpolation only — no access to the
  analytic field for vertex refinement or saddle disambiguation, which is
  where our accuracy comes from. Using it would be more glue than
  algorithm.
- **`sdfu`** is unmaintained (2021). **`bevy_smud`** pins bevy ^0.16 and
  is flat screen-space 2D; useful later only as a WGSL reference corpus.

Net hand-rolled surface: ~90 lines of textbook primitive distance
formulas plus ~250 lines of marching squares — all pure, all proptested.

## Rendering follow-up: extrusion of the same field

A 2D SDF extrudes into an *exact* 3D SDF with the standard two-line
`opExtrusion` pattern, so the future raymarched render path (M10+)
evaluates the **same** `ShapeDef` tree in WGSL — each variant is a few
shader ops — and inherits the 2.5D layer-depth look with correct depth
(and therefore shadow maps) from a fragment-depth write. Until then the
derived-mesh extrusion of the contoured field renders identically to
today's bodies.

## What this buys beyond the cut tool

Analytic circles under uniform scale (removes the polygonize-on-scale
fallback for that case), smooth unions/fillets Algodoo cannot do, CSG undo =
drop a node, and M12 magnetism becomes direct field evaluation over the same
trees — M12 shrinks to "more leaves + field-driven forces".

## Milestone impact

- **M9** = SDF core: tree + eval/AABB, contouring, `polygonize` dispatch,
  cut tool on `Subtract`, area-conservation (tolerance) + component-count
  proptests, undo restoration.
- **M10** unchanged (toon material over the derived meshes).
- **M12** reduced in scope as above.
