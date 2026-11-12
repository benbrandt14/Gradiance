# Rigid rod (salvaged from PR #194)
Phase 2 · "Decide PR #194 (rigid rod + elastica flexure spike): salvage the
`Capsule` leaf and the Spring/Strut split into briefs" · status: done

## Goal

A **Rod** tool draws a rigid 1-D rod between two points: a new analytic
`ShapeDef::Capsule` leaf body whose ends attach to whatever is under them by
**hinges** (or stay free over empty space). The current strut tool, which
makes a spring, is renamed **Spring**; behaviour unchanged. One gesture
commits one undoable step.

## Read first

- PR #194's branch `claude/rigid-rod-flexure-spike-lzfxva` (fetch it; read,
  don't merge): `src/domain/shape.rs`, `src/geometry/sdf.rs`,
  `src/geometry/polygonize.rs` (the Capsule arms), `src/interaction/tools/rod_tool.rs`,
  `src/command/rod_cmd.rs`. It predates the crate split, so paths map to
  `crates/gradiance-<layer>/src/…`.
- `docs/sdf-geometry-decision.md` — how a new analytic leaf is added.
- `crates/gradiance-interaction/src/tools/strut_tool.rs` and
  `connector_tool.rs` — today's strut (spring) and hinge connectors.
- `crates/gradiance-command/src/dispatch.rs` — the `command_intents!` table.

## Facts (orchestrator, 2026-10-25)

- **Why #194 is salvaged, not rebased:** it is pre-workspace-split (every
  path is `src/…`, 55 files, +3.5k lines); it reintroduces
  `JointKind::Weld`, which M20 removed on purpose (weld = merge into one
  body, or make static — `docs/roadmap.md` M20); and its "save format v6"
  is a different v6 from main's (the SI flip). It is closed with a pointer
  here and to the elastica follow-up.
- #194's Capsule leaf is reported exact (SDF + polygonize + avian
  `capsule_endpoints` collider) and was tested there — port it with its
  tests.
- **Fixed rod ends:** do not bring back a weld joint. A rod fixed to a body
  is a merge (`MergeIntent`, the SDF union the weld tool already does); offer
  only hinge/free ends in this brief and record "fixed end = merge" in the
  Outcome as the follow-up shape.
- #194's atomic `SpawnRodCommand` (capsule + end joints in one undo step)
  is the right shape for invariant 2. *Unverified:* whether main already has
  a multi-entity spawn command you can reuse instead (check `command/`).

## Approach

1. `ShapeDef::Capsule {length, radius}` (or #194's field names) through the
   whole derived path: SDF, polygonize, collider, mesh, snapping features
   (ends + axis; extend `geometry::snap_features` rather than tessellation
   points), CSG, persistence (serde-additive; format stays v6).
2. Rename the strut tool to Spring (user-facing label + tool id; keep the
   hotkey; `JointKind::Spring` is untouched).
3. Rod tool: drag a→b; on release, one command spawns the capsule and a
   hinge at each end that lands on a body. Reflect-register the new intent.
4. Thin UI: a toolbar entry using an existing glyph via `ui::fonts::glyph`.

## Out of scope

Elastica / flexure (`JointKind::Elastica`, #194's buckling solver) — a
separate spike after Phase 5 units; weld/fixed ends; rod inspector beyond
what the body inspector already shows; `docs/state.md`.

## Done when

- Geometry tests: capsule SDF sign/distance, polygonize area vs analytic.
- Integration: a rod drawn between two boxes spawns one capsule + two
  hinges; one undo removes all three; a rod over empty space has no joints;
  RON round-trip.
- Goldens unchanged except new content. Gates green (workspace via CI if
  disk is tight).
- Roadmap: a rod line under M20, and #194 noted as salvaged. Outcome filled.

## Outcome

**Status: done** — PR #221 (`claude/2-3-rigid-rod`).

What landed:

- **`ShapeDef::Capsule { half_length, radius }`** (#194's field names, now
  metres): the segment `(±half_length, 0)` inflated by `radius`. Through the
  whole derived path: exact SDF + tight AABB (`geometry::sdf`), exact
  stadium polygonize (two `CIRCLE_SEGMENTS/2` cap fans, CCW, every vertex on
  the zero level), avian `Collider::capsule_endpoints` (`body_sync`), mesh via
  polygonize (no render change needed), CSG for free (it is a leaf),
  uniform scale keeps it a capsule (non-uniform falls to the polygon path
  like a circle), `kind_label` "rod", inspector read-only line, RON
  serde-additive (format stays v6; added to the persistence proptest).
- **Snap features** — a new `SnapOutline::Capsule { a, b, radius }` read
  analytically (bare or under `Placed`), not from tessellation: the two
  **end centres** as vertices (where hinges go), the axis midpoint, the
  nearest point on the **axis** and on the **surface** as edges, and
  tangents from a draft reference to either end cap filtered to the outer
  half (`capsule_tangent_points`). `tangent_radius` and alignment extents
  (`align::shape_extents`) are exact for capsules too.
- **Spring rename**: `ToolState::Strut` -> `ToolState::Spring`,
  `strut_tool.rs` -> `spring_tool.rs` (`SpringDraft`), palette label
  "Spring", joint inspector "Spring (spring-damper)", hotkey `T` kept, icon
  `tool_strut.png` kept (it is a spring picture). `JointKind::Spring`, the
  scripting `spring` verb and the test names about the joint are untouched.
- **Rod tool** (`ToolState::Rod`, key `L`, palette icon `wire.png` — an
  existing two-eyelet link picture; the palette is PNG icons, not glyphs,
  so no `ui::fonts::glyph` entry was needed). Drag a->b; each end hit-tests
  with `ToolWorld::attachment_bodies_at` (exact containment, then SDF
  distance within 8 screen px — ported from #194) and gets a **hinge**
  (`JointCommon::default()`, so 2-2's `break_force`/`broken` come along);
  free ends stay free. Rod density 5 kg/m² (#194's mass-ratio rationale),
  radius 0.025 m, dark fill.
- **Atomic command**: no existing command spawns a body *and* joints that
  reference it from fresh records (`SpawnCommand` is one record,
  `DuplicateCommand` clones existing bodies), so a new
  `SpawnRodIntent { body, joints }` -> `SpawnRodCommand` (one
  `command_intents!` row; reflect-registered by the row). Undo is
  snapshot-based, so the command only validates then applies.

Tests: geometry (capsule SDF sign/distance/AABB, stadium area vs analytic
< 1 %, snap/tangent features), rod_tool unit tests, `tests/it/rod.rs`
(two boxes -> 1 capsule + 2 hinges + capsule collider, one undo removes all
three, redo restores; empty space -> no joints; one end -> one hinge; RON
round-trip). Goldens unchanged.

Follow-ups (not done, by scope):

- **Fixed end = merge**: a "fixed" rod end should be a `MergeIntent` of the
  rod with the body (SDF union), not a joint. Not offered here.
- No `spawn-rod` script verb (the intent is reflect-registered, so it is a
  one-row registry addition).
- No rod-specific inspector (length/thickness editing) — the body inspector
  shows a read-only line.
- Elastica/flexure: separate spike after Phase 5 units (unchanged).
- The attach tolerance is screen-relative; headless `CameraScale` defaults
  to 1 m/px, so integration tests set `CameraScale(0.01)`.

