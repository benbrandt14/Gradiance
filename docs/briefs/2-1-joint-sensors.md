# Joints you can plot
Phase 2 · Tracers & plotters · "Next: joint sensor sources (length/angle as
`SignalSource`s so joints plot again)" · status: ready

## Goal

A joint's live state is a signal: its **length** (distance between its two
world anchors) and its **angle** (relative rotation of the two bodies, minus
the authored rest angle) can be bound, plotted, and read by scripts exactly
like a body's speed. A selected joint's inspector shows those sensor ports
with the same ▸plot toggle bodies have.

## Read first

- `crates/gradiance-domain/src/signal.rs` ≈L20–50 — `SignalSource` (and its
  `bus_name`).
- `crates/gradiance-signal/src/lib.rs` ≈L257–310 — `read_source`, the one
  shared reader.
- `crates/gradiance-physics/src/queries.rs` — the read facade
  (`PhysicsQueries`); CLAUDE.md: "every new physics quantity gets a query as
  it lands".
- `crates/gradiance-domain/src/joint.rs` ≈L265–300 — `JointDef` (`body_a`,
  `body_b: Option`, `anchor_a/b`, `rest_rot_a/b`).
- `crates/gradiance-ui/src/ports.rs` — the body sensor-port table the
  inspector renders.

## Facts (orchestrator, 2026-10-25, main `14cb9c2`)

- `SignalSource` variants are body-keyed by `StableId`; `Named(String)` is
  bus-only. Adding variants forces match arms in `signal/lib.rs` (15 uses),
  `ui/node_graph.rs` (45), `ui/signals.rs` (33), `ui/ports.rs` (9),
  `ui/plot.rs` (2) — the compiler lists them.
- `SignalSource` is persisted inside `SignalBindings` (scene content). New
  variants are additive in RON; no format bump needed, but check the golden
  replay files don't change.
- Joints are authored entities with a `StableId` and a `JointDef`; a world
  pin has `body_b = None` (anchor_b is then a world point — *unverified*:
  confirm in `physics/joint_sync.rs`).
- Length and angle are computable from the two bodies' `Transform`s and the
  `JointDef` alone — no need to read avian's joint entity. Put the math in a
  pure function (geometry or domain) and expose it through
  `PhysicsQueries` (e.g. `joint_length(entity)`, `joint_angle(entity)`) so
  scripts and plotters read it from the facade.

## Approach

1. Pure math: `joint_state(def, pose_a, pose_b?) -> (length, angle)` with
   unit tests (hinge at rest = 0 rad; slider extension; world pin).
2. Facade reads on `PhysicsQueries` returning typed units (`Length`,
   `Angle`, as the existing reads do with `gradiance-units`).
3. `SignalSource::JointLength(StableId)` / `JointAngle(StableId)`; arms in
   `read_source`; `bus_name`; UI arms (labels/units; the node-graph palette
   only if it is a table entry — otherwise leave a TODO in the Outcome).
4. Joint inspector: the two ports with ▸plot, reusing `ui/ports.rs`'s
   pattern. Thinnest UI that exercises it (orchestration.md future intent:
   invest in the op surface, not chrome).
5. Script read: if `(signal-get …)` already resolves any bound source by bus
   name, a test showing it is enough; don't add a verb.

Invariants: read-total (no writes); 5 — joint state is derived, never saved;
only the *binding* is persisted. Future intent: reads go through
`physics::queries`.

## Out of scope

Breaking limits / joint force (brief 2-3), new actuator kinds, node-graph
layout work, joint inspector redesign, `docs/state.md`.

## Done when

- Pure unit tests for the joint-state math.
- Integration test: a hinge between two bodies, bind `JointAngle` to a plot
  sink, step with one body rotated; the bus value matches the rotation.
  Same for `JointLength` on a slider or spring.
- Golden replay files unchanged (or changed only by explicit new content).
- Gates green (workspace tests via CI if disk is tight; say so).
- `docs/roadmap.md` plotters bullet updated. Brief Outcome filled.

## Outcome

*(worker fills in)*
