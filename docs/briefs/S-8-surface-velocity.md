# Surface velocity (conveyor bodies)
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("material velocity") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A body has an optional surface speed. Bodies that rest on it are carried
along its surface as on a belt, without the body itself moving.

## Read first

- `docs/bevy20-notes.md` — avian 0.8 notes.
- `crates/gradiance-physics/src/body_sync.rs` — where collider derived
  data is built.
- `crates/gradiance-domain/src/fluid.rs` — the optional authored component
  pattern.

## Facts

- *Unverified:* avian 0.8 contact modification hooks and a per-contact
  tangent speed. **First step:** confirm the API in the avian 0.8 source.
  If it is absent, stop and return the brief with the finding. Do not
  emulate it with forces.

## Approach

- Authored `SurfaceSpeed(Velocity)` (m/s along the outline, positive =
  counter-clockwise), inspector row, `(set-surface-speed i v)`.
- One collision hook sets the contact tangent speed.

## Out of scope

Per-edge speeds. Animated belt visuals.

## Done when

- Behaviour test: a box on a 1 m/s static conveyor reaches 1 m/s ± 5 %.
- fmt, clippy, tests, doc gate green.

## Outcome
