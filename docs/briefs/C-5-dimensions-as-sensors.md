# Dimensions as live sensors
Phase C · roadmap § "Unified plan" · status: ready (after C-4)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 5; owner: dimensions publish to the bus). Docs only.

## Goal

A Dimension tool places **associative** dimensions — linear between two
points, angular between two edges, radial on a circle — whose points are
anchored to bodies (by `StableId` and a body-local point) so they follow
the bodies in edit and in play. Each dimension is a behaviour node that
**publishes its value to the signal bus** under its name: plottable,
bindable, readable by scripts. A label draws on the canvas with the value
in the display unit.

## Read first

- `docs/interaction-feel.md` — row **CAD**.
- `crates/gradiance-domain/src/node.rs:86` — `NodeKind` (Tracer, Thruster, Laser).
- `crates/gradiance-signal/src/lib.rs:266` — `publish_lasers` (the publish pattern) and `docs/briefs/Q-4-laser-and-joint-blocks.md` (a node block over bus names; free default names).
- VectorCraft `crates/tools/src/dimension.rs`, `crates/doc/src/dimension.rs` at `9fff48d`.

## Facts

- Nodes are authored scene content, copy with their bodies, and are
  undoable (Tracer precedent); a node "attached to a body" already exists.
- A dimension has two attachments, unlike today's nodes (one target body).

## Approach

- Feel: **CAD**. `NodeKind::Dimension(Dimension { kind, a: Anchor, b: Anchor, name, offset })`
  where `Anchor = { body: Option<StableId>, local: Vec2 }` (`None` = world).
- Derived value per step in one allocation-free system; publish `<name>`
  (metres or radians, the quantity catalog's units).
- Tool: hover an edge (linear), a corner (angular) or a circle (radial) and
  drag out; or click two points. One `spawn-node` undo step; `(dimension …)`
  operation; Adjust Last on the offset.
- A node-canvas block like the laser's (Q-4).
- `FORMAT_VERSION` bump.

## Out of scope

Driving geometry from a dimension (that is a constraint; the CAD-kernel
Horizon item). Dimension styles beyond one look.

## Done when

- Behaviour test: a dimension between two bodies on a slider reads the
  slider's travel within 1e-4 m while it plays; the bus value equals it.
- Undo matrix and loader fuzz cover the new node (no new list).
- Gates green; screenshot on the PR.

## Outcome
