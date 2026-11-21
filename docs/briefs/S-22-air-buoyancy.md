# Air buoyancy
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("air buoyancy") · status: ready

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

With air on, every dynamic body gets an upward force of air density × g ×
area at its centroid, so a body lighter than air rises (a balloon).

## Read first

- `crates/gradiance-physics/src/air.rs` — the drag pass and `air_density`.
- `crates/gradiance-physics/src/buoyancy.rs` — the region version (N-7).

## Facts

- `SimSettings.air_density` default is 1.2 (areal units by the units
  decision). Normal bodies (density ≥ 2) barely notice; a body below 1.2 floats.
- Area comes from the derived outline the drag pass already reads.

## Approach

- Add the term in the existing air system; a `SimSettings.air_buoyancy`
  switch (scene content, default off so old scenes behave the same).

## Out of scope

Air density varying with height.

## Done when

- Behaviour test: a body of density 0.6 rises with net accel g × (1.2/0.6 − 1) ± 3 %.
- Gates green.

## Outcome
