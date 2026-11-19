# Coordinate frames — decision and phased plan

**Status: decided (session best estimate, 2026-11-19); the owner may
reverse it.** Decided under the owner ruling of 2026-11-17 ("Leave non
interactive changes to the thread here. Go with your best estimates"):
an architecture option is made by the session and recorded, not asked. The
record is the `frames-option` entry in `docs/review/archive.ron`. Brief:
`docs/briefs/5-5-frames-decision.md`. Its companion, the interaction-layer
face of the same choice, is `docs/curvilinear-tools-decision.md`.

Nothing in this document is built yet. The phases below become briefs at
the next planning stage (3D-0 in `docs/state.md`), in the order given.

## The question

Should authored and derived quantities be expressible in frames other than
the world frame (a body's local frame, a joint's frame, a user-defined
datum or sketch plane), and if so, where does the frame transform live?

The consumer that decides it is **multiple simulation planes, then real 3D**
(`docs/orchestration.md` § Future intent 2). A simulation plane is a frame
that bodies are held on. CAD datums and sketch planes are a second
consumer of the same seam (`docs/roadmap.md` § Horizon, CAD kernel
front-end). The recommendation has to make the first one cheaper, not only
the second.

## Terms

- **Frame:** a rigid transform (an origin and an orthonormal basis). It
  moves points and keeps distances and straight lines.
- **Plane frame:** a frame that puts a 2D plane into 3D. It lifts a
  plane-local `(x, y)` and an offset along the normal to a world `Vec3`,
  and projects back.
- **In-plane frame:** a 2D frame inside one plane: an origin and a
  rotation. `PosRot` already is one, and so is the grid's origin and
  rotation.
- **Chart:** a coordinate system on a plane, such as Cartesian, polar or
  isometric. A chart can be non-linear (polar). Charts are the subject of
  the curvilinear note, not of this document.

## What the code does today

The world is one plane: world XY at `z = INTERACTION_PLANE_Z = 0`. Bodies
author a `PosRot` (plane-local `Vec2` and a rotation about +Z) and a
`DepthBand` (offsets along −Z). The physics engine is `avian2d`. Rendering
and the camera are 3D, so a 2D↔3D seam already exists, but it is spread
over the code rather than named.

Three facts change the options as the brief stated them:

1. **There is no `WorldScale` type, and `units::world` is not a frame
   seam any more.** The units decision placed the frame transform "at the
   `WorldScale` seam". Since the P2 flip the world is SI. `PIXELS_PER_METER`
   is now read only as the camera's orthographic scale
   (`crates/gradiance-render/src/lib.rs:117`, `src/review.rs:471`). The
   value between authored state and the engine is the identity. A frame
   cannot compose at a scale that is not on the path. The seam that is on
   the path is the 2D↔3D lift: `PosRot::from_transform` / `apply_to`
   (`crates/gradiance-core/src/units.rs:51-66`), which 36 call sites in 17
   files already use.
2. **In-plane frames already exist and work.** Body-local frames (shape
   vertices are centroid-relative, joint anchors are body-local:
   `crates/gradiance-domain/src/joint.rs:336-338`), the grid's user frame
   ("Align grid to body", `GridSettings.origin`/`rotation`), the axis-lock
   basis (`interaction::gesture::grid_basis`) and the oblique alignment
   frame (`geometry::align::AlignFrame`) are all in-plane frames. None of
   them needs a new seam. They need one type, so that they stop travelling
   as `(origin, rotation)` argument pairs.
3. **PR #208 already designed the plane frame.** Its `PlaneFrame`
   (`core::units`, origin + in-plane basis, lift/project/spin, one
   instance `PlaneFrame::XY`) and its spine — "the authored and queried
   world stays plane-local 2D; three dimensions exist only between the
   physics sync systems and the engine" — are independent of its engine
   choice. That PR stays parked. Its frame model is input here, and is
   ported, not merged.

## Inventory — single-plane assumptions and frame-like seams

Method: a scan of `crates/` and `src/` for the patterns that build or
drop the third axis by hand (`.extend(0.0)`, `.extend(INTERACTION_PLANE_Z)`,
`translation.truncate()`, `Quat::from_rotation_z`, `EulerRot::ZYX`,
`translation.z`, `Vec3::Z`, `ray.direction.z`), then a read of each hit,
plus the 2D gizmo calls, the shaders and the engine. Line numbers are at
4b-5 (`2a195f8`).

**Size:** the scan finds **60 sites in 23 Rust files**. Six of them draw
the grid through the named `INTERACTION_PLANE_Z` and three are the
`PosRot` seam itself; the other **51 are unrouted**. On top of those:
**1 shader line** (`plane.wesl:123`), **78 `*_2d` gizmo calls in 7 files**
that draw in world XY at z = 0 implicitly, and **the engine** (`avian2d`
in 8 manifests and 27 source files). That is 24 rows below: 16
single-plane assumptions (rows 1–16) and 8 frame-like seams (rows 17–24).

| # | Site (file:line) | What it assumes | Class |
|---|---|---|---|
| 1 | `crates/gradiance-interaction/src/camera.rs:313-323` `plane_point` | cursor ray ∩ `z = 0`, a literal, not `INTERACTION_PLANE_Z` | unrouted: picking |
| 2 | `crates/gradiance-interaction/src/camera.rs:218`, `:277` | pan in camera XY; zoom anchor `extend(rig.focus.z)` | unrouted: camera |
| 3 | `crates/gradiance-interaction/src/camera.rs:58-87`, `:305`; `crates/gradiance-render/src/lib.rs:120` | orbit yaw about world Y, home view looks along −Z | unrouted: view |
| 4 | `crates/gradiance-ui/src/view_cube.rs:84-105` | the "F" face is +Z | unrouted: view |
| 5 | `src/review.rs:477` | review camera focus `extend(0.0)` | unrouted: camera |
| 6 | `crates/gradiance-render/src/plane.wesl:123`; `plane.rs:8` | the authoring-plane line is world `z = 0` | unrouted: shader |
| 7 | `crates/gradiance-render/src/plane.rs:176` | ground normal = in-plane normal `extend(0.0)`; grounds sweep along ±Z | unrouted: render |
| 8 | `crates/gradiance-render/src/scenery.rs:54`, `:182`, `:204-209` | back plane normal `Vec3::Z`, placed at `z = −far − offset` | unrouted: render |
| 9 | `crates/gradiance-render/src/decorations.rs:17-19`, `:40`, `:64-67` | border/marker z from the band; heading via `EulerRot::ZYX` | unrouted: render |
| 10 | `crates/gradiance-physics/src/joint_sync.rs:103` | world pin anchor `extend(0.0)` | unrouted: physics |
| 11 | `crates/gradiance-physics/src/grab.rs:47-48`; `motor.rs:34-41`; `fields.rs:176`, `:218`, `:225` | local↔world by `extend(0.0)`/`truncate()`; angle by `EulerRot::ZYX` | unrouted: physics |
| 12 | `crates/gradiance-signal/src/lib.rs:283`, `:323`; `crates/gradiance-render/src/tracer.rs:92`, `:199`, `:225`; `debug_viz.rs:148`, `:174`, `:282`, `:319`; `crates/gradiance-ui/src/probe.rs:117`; `ports.rs:111` | a body position is `translation.truncate()` | unrouted: reads |
| 13 | `crates/gradiance-interaction/src/snap.rs:238`, `:359-367`; `tools/handles.rs:136`; `tools/context.rs:692`, `:716`; `indicators.rs:114`; `node_edit.rs:48`; `crates/gradiance-ui/src/context_menu.rs:302`, `:863`; `crates/gradiance-render/src/debug_viz.rs:27-28` | body-local ↔ world by hand: `affine.transform_point3(v.extend(0.0)).truncate()` | unrouted: local frame |
| 14 | 78 `*_2d` gizmo calls: `render/joint_viz.rs` 28, `render/debug_viz.rs` 22, `interaction/indicators.rs` 13, `render/tracer.rs` 6, `interaction/tools/context.rs` 4, `tools/handles.rs` 3, `interaction/pack.rs` 2 | overlays draw in world XY at z = 0 | unrouted: overlay |
| 15 | `Cargo.toml:16` `avian2d`; 8 member manifests; 27 source files | the engine is 2D: `Vec2` velocity, scalar spin, 2D colliders | engine |
| 16 | `crates/gradiance-domain/src/depth.rs:75` `z_front = −near`; `crates/gradiance-geometry/src/extrusion.rs:56-61`; `crates/gradiance-render/src/extrude_sync.rs:23` | depth runs along world −Z | unrouted: depth |
| 17 | `crates/gradiance-core/src/constants.rs:17` `INTERACTION_PLANE_Z` | the plane's offset, named once | seam (named) |
| 18 | `crates/gradiance-render/src/grid.rs:32`, `:76`, `:86-87`, `:123-124` | grid drawn through `INTERACTION_PLANE_Z` | routed through 17 |
| 19 | `crates/gradiance-core/src/units.rs:51-66` `PosRot::from_transform`/`apply_to` (36 call sites, 17 files) | the de facto 2D↔3D lift and projection | seam (unnamed plane frame) |
| 20 | `crates/gradiance-domain/src/depth.rs:27-32`, `:95` `DepthBand`, `bits()` | depth as offsets along the plane normal; collision layer ≡ depth | seam (plane-local) |
| 21 | `crates/gradiance-domain/src/settings.rs:43-46` `GridSettings.origin`/`rotation`; `crates/gradiance-ui/src/context_menu.rs:139-142` "Align grid to body" | the user's in-plane frame (roadmap V5) | seam (in-plane frame) |
| 22 | `crates/gradiance-interaction/src/gesture.rs:108` `grid_basis`; `crates/gradiance-geometry/src/snapping.rs:94-109`, `:179`; `crates/gradiance-geometry/src/align.rs:72` `AlignFrame` | snap and lock in the grid's frame, as `(origin, rotation)` pairs | seam (in-plane frame) |
| 23 | `crates/gradiance-domain/src/joint.rs:336-338`; centroid-relative shape vertices | body-local frames | seam (in-plane frame) |
| 24 | `crates/gradiance-units/src/world.rs:19` `PIXELS_PER_METER`; `crates/gradiance-core/src/constants.rs:11` `GRAVITY: Vec2` | camera scale only (fact 1); gravity is plane-local | stays |

What the inventory says: the in-plane frames (rows 21–23) are healthy and
need only a common type. The 2D↔3D seam exists (row 19) but has no name,
and 51 sites, the overlay gizmos and the depth direction go around it.
None of the unrouted sites is hard to move: each is one expression. The
cost of every option is mostly "route these sites", and that cost grows
with every feature that adds one.

## Options

### (a) A frame value type, composed at the 2D↔3D seam

One pure value type, `PlaneFrame` (origin `Vec3`, in-plane basis
`x`, `y`; normal = `x × y`), with lift (`point`, `dir`, `spin`,
`transform`) and project (`project`, `project_dir`, `unspin`, `pose`).
One instance today, `PlaneFrame::XY`, seeded from `INTERACTION_PLANE_Z`.
`PosRot` stays the in-plane frame type. Authored and queried state stays
plane-local 2D. Only sync systems, picking, the camera and the renderer
lift.

The brief named this "a `units`-level type composed at `WorldScale`".
Fact 1 moves it: the type composes at the lift, and it lives in
`gradiance-core` beside `PosRot`, which it transforms. `gradiance-units`
stays a quantity crate. Like `units`, it is a pure value type in a bottom
crate that every layer can read.

- **Cost:** small. The type and its tests exist in #208 (about 280 lines,
  six tests, including the bit-exact `xy_frame_is_pos_rot` equivalence).
  Routing the 51 sites, the shader line and the gizmo calls is mechanical
  work over about 25 files. No command, intent, op or record changes.
- **Unlocks for simulation planes:** a second plane is a second *value*.
  Physics, render and interaction already lift through the frame, so they
  do not change. The engine decision (#208's question) becomes a change
  behind the lift. It is not a crate-wide edit. A CI ratchet can enforce
  "no new single-plane assumptions" (Future intent 2) for the first time.
- **Save format:** no change. Plane-local `PosRot`/`DepthBand` are already
  what is saved.
- **Does not give:** planes or datums with identity, undo, names or
  inspector rows. A plane is a value in code, not a thing in the document.

### (b) Frames as authored entities (datums) that bodies reference by `StableId`

A `Datum` (or `SimPlane`) entity with a `StableId` and an authored pose.
Bodies carry `OnPlane(StableId)`. Commands, records, undo, the inspector,
ops and verbs exist for datums.

- **Cost:** large. It needs a new authored archetype, a scene record kind,
  `command_intents!` rows (create, move, delete, assign), undo capture, an
  inspector, ops and verbs, a render glyph, and an order of evaluation for
  frames attached to moving bodies (a datum on a body must resolve before
  the bodies on that datum sync). It also still needs (a): an authored
  datum's pose has to be lifted somewhere. Today it has no consumer: no
  feature authors a second plane or a sketch plane.
- **Unlocks for simulation planes:** planes as document content: named
  planes, per-plane settings, moving planes (a plane carried by a body),
  and cross-plane joints that reference planes by `StableId` (invariant 3).
- **Save format:** a bump (allowed freely, ruling 2026-11-16): a new record
  kind and a body field.

### (c) Defer until the 3D engine decision

- **Cost:** none now. But the unrouted sites grow while "don't add new
  single-plane assumptions" stays an honour rule. Deferring is also
  circular: the engine decision needs to know how planes are represented.
  #208 had to invent `PlaneFrame` before it could swap the engine. That
  shows the seam comes before the engine, not after it.
- **Unlocks:** nothing. The 3D planning stage starts with the inventory
  above, larger.
- **Save format:** no change now. Later, whatever the engine decision
  forces.

## Recommendation

**(a) now; (b) for planes and datums when their first consumer is
briefed; not (c).**

Build the plane frame as a value type at the 2D↔3D seam, route every
unrouted site through it, and add a CI ratchet so the count only goes
down. Keep all authored, saved, queried and scripted state plane-local
2D. Give planes and datums identity (option (b)) only when a brief
authors a second plane or a sketch plane. Reference them by `StableId`
then, not by index.

Why:

- It is the only option that makes the 3D direction cheaper at once. Each
  routed site is a site the engine swap and the second plane do not
  touch. (b) costs more and still contains (a). (c) costs nothing and
  leaves the work for later, at a larger size.
- It keeps the property that makes 2.5D fast to author: the plane is
  implicit. Tools, intents, scripts and the save format keep `Vec2`. #208
  reached the same spine for the same reason.
- It changes no invariant. Frames are derived geometry. No new mutation
  path, no new authored state, no format bump.

Trade-offs that this choice accepts:

- **No datums yet.** CAD sketch planes wait for (b). The grid's
  "Align grid to body" frame keeps covering the in-plane case.
- **The ratchet is a pattern scan, not a type guarantee.** A site can hide
  from the regex. This is the same trade as the glyph scanner in
  `docs/ui-design.md`, and it is cheap to tighten.
- **Different from #208 on the save format.** #208 saves a
  `SimPlaneId(u8)` index on every body now, always 0. This decision does
  not. The format may change freely, so a field that is always 0 costs
  more than the bump that adds it later. An index also breaks invariant 3
  (identity is `StableId`; deleting plane 1 renumbers plane 2).

## Which seams change, which stay

**Change**

- `PosRot::from_transform`/`apply_to` delegate to `PlaneFrame::XY`
  (`pose`/`transform`). Their callers do not change.
- `camera::plane_point` intersects the active plane frame, not `z = 0`.
  It is the one picking seam (row 1).
- Overlay gizmos draw through one helper that lifts plane-local 2D through
  the frame (rows 14, 18). The 78 `*_2d` calls move to it.
- Body-local ↔ world conversions go through `PosRot` methods
  (`to_world(local)`, `to_local(world)`), not hand-built affines (row 13).
- `DepthBand` keeps its meaning but is read as "offsets along the plane
  normal". `z_front` becomes `frame.point(p, −near)` at the render and
  physics sync sites (row 16).
- The plane shader takes the authoring plane as a uniform (row 6).
- `GridSettings.origin`/`rotation` become one `frame: PosRot`, and the snap
  functions take it (rows 21–22). This is a workstation-config format bump.

**Stay**

- Typed quantities and the `PhysicalQuantity` catalog: frame-agnostic,
  as the units decision held open.
- `physics::queries` returns plane-local 2D. Readers never see a `Vec3`.
- Intents, commands, ops, verbs and records carry plane-local SI `Vec2`.
- `DepthBand::bits()`: collision layer ≡ visual depth, per plane.
- `units::world`: a camera scale only. It is not a frame seam (fact 1).
- `INTERACTION_PLANE_Z`: the default plane's offset, read only where
  `PlaneFrame::XY` is defined.
- The camera rig's orbit is a view. It may stay world-relative. Only
  "home" and the view cube's "front" become the active plane's normal.

## Phased plan

Each phase leaves fmt, clippy and test green. No dates. F1–F3 are small
briefs that fit before the 3D planning stage. F4–F5 belong to it.

0. **F0 — this decision** (brief 5-5). The inventory, the options and the
   record in `docs/review/archive.ron`.
1. **F1 — the type and the ratchet.** Port `PlaneFrame` and its tests from
   #208 into `gradiance-core` (by copying, not by merging the PR).
   `PlaneFrame::XY` is seeded from `INTERACTION_PLANE_Z`, and `PosRot`'s
   two methods delegate to it. Add a `tests/boundaries.rs` ratchet: the
   count of the inventory's scan patterns outside the frame module must
   not rise above the committed number. No behaviour change, no format
   change.
2. **F2 — route the 2D↔3D sites.** Picking, camera home, review focus,
   overlay gizmos, the plane shader line, the back plane, decorations,
   depth extrusion, the physics pin, grab, motor and fields, and the
   position reads (rows 1–14, 16) go through the frame or `PosRot`
   helpers. The ratchet number falls to near zero. Add an `ActivePlane`
   resource (always `PlaneFrame::XY`) for picking, the overlays and the
   camera, so that tools read the plane from one place. No behaviour change.
3. **F3 — one in-plane frame type.** `GridSettings` keeps a
   `frame: PosRot`. The snap, lock and alignment functions take it in
   place of `(origin, rotation)`. The grid's chart functions land here
   (`docs/curvilinear-tools-decision.md`). Workstation-config format bump.
4. **F4 — planes as authored entities** (3D planning stage, with the engine
   decision). This is option (b) for simulation planes: a plane entity with
   a `StableId` and a `PlaneFrame`, `OnPlane(StableId)` on bodies (absent =
   the default plane), records, commands and verbs. Physics lifts each
   body through its plane's frame. The engine decision chooses how a body
   is held on its plane (#208: locked axes for axis-aligned planes, a
   joint for tilted or moving ones). Format bump.
5. **F5 — datums.** CAD sketch planes and user frames reuse F4's entity.
   The grid frame may then reference a datum by `StableId` in place of a
   copied pose.

## Inputs read

- `docs/units-decision.md` § 4 (coordinate frames on a dedicated path),
  § Forward compatibility, § Phasing (P2·flip).
- `docs/roadmap.md` § Horizon, M18 (curvilinear line), V5 (local-frame
  grids).
- `docs/orchestration.md` § Future intent 2.
- PR #208 (draft, parked, read only): `crates/gradiance-core/src/units.rs`
  `PlaneFrame`, `crates/gradiance-domain/src/plane.rs` `SimPlanes`/
  `SimPlaneId`, `crates/gradiance-physics/src/plane.rs` `PlaneConstraint`.
