# Air buoyancy
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("air buoyancy") · status: done

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

Done 2026-11-23 (PR on the S-21 branch).

- `SimSettings.air_buoyancy: bool` (`#[serde(default)]`, off): scene
  content, so it is saved and a settled edit is one undo step. The Air
  section of Settings > Simulation has the checkbox (label from the
  string table); `sim-set "air_buoyancy" 1` sets it.
- `physics::air::air_lift` (pure) and `apply_air_buoyancy`, beside the
  drag pass in `FixedUpdate`: `−g × air_density × area` at the outline's
  centroid. `wake_on_air_buoyancy` wakes sleeping bodies when the switch
  turns on, so a light body asleep on the floor rises.
- The area is the exact `SubmergeOutline`, not the drag hull (decision
  `air-buoyancy-area-from-outline`). Gravity is the world's (decision
  `air-buoyancy-world-gravity`).
- `reflect_path::read_path` now reads a flag as 1 or 0, so a bool sim
  path resolves in `sim-get` and in the registry validation.
- Tests: `tests/it/air_buoyancy.rs`. A body of density 0.6 rises at
  g × (1.2/0.6 − 1) and one of 2.4 falls at −g/2, both within 3 %; off,
  the light body falls at g. Unit test for `air_lift`. The format golden
  sets the switch; the replay goldens and the settings-grid snapshot
  gain the new field.
