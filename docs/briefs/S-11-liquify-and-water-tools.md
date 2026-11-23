# Liquify and water tools
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("liquify", "add water") · status: ready (after S-10) · feel: Algodoo (play) (`docs/interaction-feel.md`)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

"Liquify" turns a body into particle water of the same area. A water tool
fills a drawn region. "Drain" removes all water. Each is one undo step;
undo brings the body back.

## Read first

- `docs/particle-fluid-decision.md` (S-10) — the particle store and its coupling.
- `crates/gradiance-command/src/property.rs` — the edit path.
- `docs/scripting.md` — the verb recipe.

## Facts

- Particles are derived (S-10). The authored record is a `WaterEmitter`
  (outline + spacing), saved with the scene. Liquify = delete body + add
  emitter, one command.

## Approach

- `body.liquify` (`(liquify i)`), `water.fill` (`(add-water path)`),
  `water.drain`. Context row, palette entry, no key.
- Emitters spawn their particles when a run opens; undo of a run restores
  them, like every derived state.

## Out of scope

Freezing water back into a body. Per-particle colour.

## Done when

- Test: liquify keeps total particle area within 5 % of the body area;
  undo restores the body exactly.
- Undo matrix covers the three operations. Gates green.

## Outcome

Done (2026-11-22).

- **Record:** `domain::water::WaterEmitter` (outline relative to the node,
  spacing 0.02 to 0.5 m, default 0.04 m), saved as
  `NodeKind::Water`. The emitter is a free node, so delete, undo, save and
  the outliner treat it like a thruster. Decision `water-emitter-is-a-node`.
- **Fill:** `physics::particles::fill_from_emitters` fills each emitter
  once as the run plays, on the lattice `geometry::lattice::fill_polygon`.
  `drop_stale_water` takes out the particles of a gone emitter and all of
  them when a run is undone. One water per scene: the first emitter sets
  the spacing (decision `one-water-per-scene`).
- **Operations:** `body.liquify` (`(liquify i)`, context row "Liquify"; it
  skips grounds and fluid regions), `water.fill` (`(add-water path)`, the
  water tool's commit), `water.drain` (`(drain)`). Each is one undo step.
- **Tool:** `ToolState::Water`, palette "Create", no key, the `cloud` icon
  (the set has no water icon). Drag a rectangle.
- **Tests:** `tests/it/water.rs` (liquify keeps the area within 5 %, undo
  restores the body exactly; fill and drain), the conformance fixture has
  a water emitter, the undo matrix covers the three operations,
  `water_tool` and `lattice` unit tests.
- **Measured:** a 1 × 0.6 m box liquifies to particles within 5 % of
  its area.

