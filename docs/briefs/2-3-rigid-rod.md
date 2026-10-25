# Rigid rod (salvaged from PR #194)
Phase 2 · "Decide PR #194 (rigid rod + elastica flexure spike): salvage the
`Capsule` leaf and the Spring/Strut split into briefs" · status: ready

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

*(worker fills in)*
