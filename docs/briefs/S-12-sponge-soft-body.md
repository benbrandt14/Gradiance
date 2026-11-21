# Sponge (soft body from a lattice)
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("make sponge") · status: ready · feel: Algodoo (play) (`docs/interaction-feel.md`)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

"Make sponge" replaces a body with a lattice of small circles joined by
springs, inside the body's outline. It squashes and recovers. One undo
step. Stiffness and lattice spacing are parameters (Adjust Last).

## Read first

- `crates/gradiance-command/src/rod_cmd.rs` — `SpawnRodCommand`, generalised to many bodies in N-5.
- `crates/gradiance-geometry/src/chain.rs` — a pure layout from parameters.
- `crates/gradiance-domain/src/group.rs` — the lattice becomes one group.

## Facts

- Every part exists: circles, spring joints, one-command multi-spawn,
  groups. No new solver. The cost is joint count: an n × n lattice has
  about 4n² springs. *Unverified* the step time at n = 12.

## Approach

- Pure `geometry::lattice(outline, spacing) -> (points, edges)` (grid
  clipped to the outline, structural + shear edges).
- `body.sponge` (`(sponge i spacing stiffness)`): delete body, spawn the
  lattice as one group, one undo step. Context row "Make sponge".

## Out of scope

A real soft-body solver. Pressure bodies. Rendering a skin over the lattice.

## Done when

- Pure test: every lattice point lies inside the outline; the graph is connected.
- Behaviour test: a sponge dropped 1 m recovers to within 10 % of its area.
- Gates green; `docs/ops.md` regenerated.

## Outcome
