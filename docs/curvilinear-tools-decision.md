# Curvilinear tools — what a tool's coordinate space is

**Status: decided (session best estimate, 2026-11-19); the owner may
reverse it.** Decided under the owner ruling of 2026-11-17 (non-interactive
choices are made by the session and recorded). The record is the
`curvilinear-edges` entry in `docs/review/archive.ron`. Brief:
`docs/briefs/5-5-frames-decision.md`. This note is the interaction-layer
face of `docs/frames-decision.md`. Read that first for the terms *frame*,
*in-plane frame* and *chart*.

## The question

Roadmap M18 (3.3): "Curvilinear abstraction: tools operate in grid
coordinates (polygon edges curve in polar grids)." It was moved out of M18
to Phase 5 because it shares a seam with simulation planes
(`docs/orchestration.md` § Phase 1).

Two questions are folded into it:

1. **In which space does a tool compute?** World, plane-local, grid frame
   or grid chart?
2. **Is an edge straight in the plane or straight in the chart?** In a
   polar grid, does a polygon edge between two clicked points curve?

## What the code does today

- Tools compute in world XY, which is also the only plane's local 2D
  space (frames inventory, rows 1 and 19).
- **Points snap in the chart.** `geometry::snapping::cartesian_snap`,
  `isometric_snap` and `polar_snap`
  (`crates/gradiance-geometry/src/snapping.rs:94-121`) take the grid's
  `(origin, rotation)` frame and its system. Polar snaps to ring × spoke
  intersections.
- **Locks use the frame, not the chart.** The axis lock in a polar grid
  uses the Cartesian directions in the grid's frame
  (`interaction::gesture::grid_basis`, `crates/gradiance-interaction/src/gesture.rs:108`).
  There is no radial or ring-following lock.
- **Edges are straight in the plane.** A polygon leaf is a list of
  vertices joined by straight segments. The SDF tree has `Box`, `Circle`,
  `Polygon`, `Capsule`, `HalfPlane` and `Csg`. No leaf holds a spiral or
  an arc edge. `geometry::polygonize` is the single discretization point.

## Options

1. **Points in the chart, edges straight in the plane.** Today's
   behaviour, now stated as the rule. Grid coordinates are a view of the
   plane-local space: snap targets, readouts and locks. They are never
   stored. Intents carry plane-local SI `Vec2`.
2. **Edges interpolated in the chart.** An edge from `(r₁, θ₁)` to
   `(r₂, θ₂)` is linear in `(r, θ)`. That is an Archimedean spiral
   segment, and a circular arc only when `r₁ = r₂`. A polygon drawn in a
   polar grid curves.
3. **Explicit curved constructors.** Edges stay straight (option 1). A
   tool option adds the curve where it is wanted: an arc along a ring
   between two points on that ring (an exact circle arc), and a
   radial/ring lock (the polar grid's own families, the chart-aware
   counterpart of the axis lock).

### Under each frames option

| Frames option | A tool's coordinate space |
|---|---|
| (a) frame value type (chosen) | The active plane's local 2D, in metres. The cursor ray meets `ActivePlane`'s frame. The grid is an in-plane frame plus a chart inside that plane. |
| (b) authored datums | The same space, but the active plane, and the grid's frame, may be a datum named by `StableId`. Intents would also name the plane. |
| (c) defer | World XY. The cursor ray meets `z = 0`. Each tool that wants a second plane later has to be revisited. |

Under every option, a chart is 2D and lives inside one plane. A tilted
plane changes the frame, not the chart. So the edge question has the same
answer on every plane, and in a future 3D editor too.

## Recommendation

**Option 1 now. Option 3 is retained as a later constructor and lock.
Option 2 is rejected.**

Tools compute in plane-local 2D. Points snap through the grid's chart.
Edges are straight in the plane. A polygon edge drawn in a polar grid does
not curve.

Why not option 2:

- **No exact geometry for it.** A spiral edge has no SDF leaf. It would
  have to be stored as a sampled polyline (it loses exactness and grows
  the save file) or be a new leaf whose distance field has no closed form.
  Both work against `docs/sdf-geometry-decision.md`.
- **Behaviour depends on a view setting.** The same two clicks would make
  a different body when the grid is polar than when it is Cartesian or
  hidden. The grid is workstation config (CLAUDE.md invariant 4), so a
  person's grid choice would change the document's geometry.
- **Scripts cannot reproduce it** without reading the grid. Today a
  script that authors the same vertices as the tool gets the same body,
  whatever grid the person has on.
- **The common wish is an arc on a ring.** Option 3 gives that one case
  exactly (a circle arc is buildable from `Circle` and `Csg`). It does not
  pay the costs above.

Why option 3 is not built now: it is a product choice that the owner would
judge in a live scene, and neither constructor exists yet. Under
`docs/orchestration.md` § Review mode, a question whose alternative cannot
be shown yet is a follow-up, not a review question. It goes to "Retained"
(Snapping/tools) in `docs/state.md` at the next planning stage.

## Which seams change, which stay

- **Change (frames F3):** one function pair per grid system in
  `gradiance-geometry`: `to_chart(frame, p) -> (u, v)` and
  `from_chart(frame, (u, v)) -> p`. The three `*_snap` functions become
  "to chart, round, from chart". The readout and a future radial/ring lock
  use the same pair. Charts stay pure math in `geometry` (no systems).
- **Change (frames F2):** `camera::plane_point` meets the active plane's
  frame. Tools read the plane from `ActivePlane`.
- **Stay:** tools are `ToolContext → (preview, commit-intent)`. Intents
  and verbs carry plane-local SI points. `ShapeDef` leaves and
  `polygonize` do not change. Snapping is still the one source of grid
  targets.
