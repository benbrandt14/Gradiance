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

Done (2026-11-22).

- **Layout:** `geometry::lattice::sponge(outline, spacing)` gives the
  `fill_polygon` points and their edges: side and upper neighbours, and
  both diagonals of each cell. Only the largest connected part stays.
- **Operation:** `body.sponge` (`(sponge i spacing stiffness)`), context
  row "Make sponge". Spacing 0 picks about six circles across the smaller
  side of the body's box (at least 0.03 m); at most 400 circles.
  Stiffness is the springs' natural frequency in Hz (the spring tool's
  unit); the context row uses 12 Hz, damping ratio 0.5. The circles keep
  the body's material, colour and depth and become dynamic. Grounds and
  fluid regions stay. Decision `sponge-stiffness-default`.
- **Command:** `rod_cmd::SpongeCommand` deletes the body and spawns the
  circles and springs through `SpawnRodCommand`, in one fresh group, one
  undo step.
- **Measured:** a 1 m box (36 circles, 110 springs) dropped 1 m squashes
  to 80 % of its hull area and recovers to 98 % (`tests/it/sponge.rs`).
  At 6 Hz it sagged to 87 % under its own weight, so the default is 12 Hz.
- **Tests:** `lattice` unit tests (inside, connected, a neck keeps the
  larger part), `tests/it/sponge.rs` (recovery within 10 %, one group,
  undo restores the box), the undo matrix and the conformance suite.

