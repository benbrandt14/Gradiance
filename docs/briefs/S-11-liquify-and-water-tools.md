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
