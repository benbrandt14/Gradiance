# Aerodynamic torque and angular drag
Phase Q (follow-ups) · N-4 Outcome "no aerodynamic torque or angular drag
(a plate keeps its attitude)" · status: done

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

With air drag on, drag turns bodies and damps their spin:

- a body whose shape is not symmetric about its centre of mass turns
  into the flow like a weathervane (a dart with a heavy nose falls nose
  first; a flag on a pole swings downwind);
- a spinning body in still air slows down (angular drag);
- the translation results of N-4 do not change: a body that does not
  spin gets the same total force as today, so the terminal-velocity
  tests pass unchanged.

The model is decided as `aero-panel-drag` (archive, planning thread): the
drag of each edge of the body's convex hull that faces the flow, at that
edge's midpoint, with the edge's own relative velocity `v + ω × p − wind`.
For a still body the sum over the facing edges equals today's
width-profile law exactly (the facing edges of a convex hull project onto
the flow-normal line as its width). A rectangle about its centre gets no torque
at any angle, so a flat plate still keeps its attitude: flutter needs
lift, which is out of scope.

## Read first

1. `crates/gradiance-physics/src/air.rs` (whole file: `DragLaw`,
   `apply_air_drag`, the cap).
2. `crates/gradiance-geometry/src/profile.rs` (`WidthProfile`, how the
   hull is used).
3. `tests/it/air_drag.rs` (the N-4 measurements that must keep passing).
4. `docs/review/archive.ron` entries `air-drag-law`, `air-drag-torque`,
   `aero-panel-drag`.

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- `DragLaw { linear, half_rho_cd, wind }` (`air.rs:42`);
  `DragLaw::force` (`air.rs:81-102`) uses one relative velocity and the
  width across it, `F = −w (c + ½ρCd|r|) r`, capped so `|F| dt ≤ m |r|`.
- `apply_air_drag` (`air.rs:111`) applies the force at the centre of mass
  (`forces.apply_force`, `air.rs:138`); it returns at once while both
  coefficients are 0 (the default).
- `AirProfile(pub WidthProfile)` (`air.rs:38`) is derived beside the
  collider in `body_sync::sync_colliders`
  (`crates/gradiance-physics/src/body_sync.rs:114`), from the same
  polygonized outline; `PROFILE_SAMPLES = 64` (`profile.rs:23`).
- `Forces::apply_force_at_point` and `apply_torque` are used by
  buoyancy (`crates/gradiance-physics/src/buoyancy.rs:267-268`).
- Decision `air-drag-torque` (archive) chose "no torque" for N-4 and named
  the centre-of-pressure torque and angular drag as follow-ups.

## Approach

- Geometry (pure): `DragPanels`, the convex hull's edges (midpoint,
  outward normal, length) in the body frame relative to the centre of
  mass, at most 64 (a decimated hull). Built where `AirProfile` is built,
  from the same outline; a half-plane gives none.
- Physics: per dynamic body, per panel with `n · r_i > 0` (the edge
  leads into the air; `r_i = v + ω × p_i − wind`), the panel force `−L_i |n̂ · r̂_i| (c + ½ρCd
  |r_i|) r_i`; sum the forces and the torques about the centre of mass;
  apply once (`apply_force`, `apply_torque`). Keep the step cap on the
  force and add the same cap on the torque (`|τ| dt ≤ I |ω_rel|`) so a light
  spinning body cannot reverse in one step.
- Replace `AirProfile` with the panels if nothing else reads it, or keep
  both if the width is still used; say which in the Outcome.
- Cost stays `O(B · P)` per step, `P ≤ 64`, no allocation.

## Tests

- `tests/it/air_drag.rs`: the seven N-4 tests pass unchanged;
  `a_dart_turns_nose_first` (a box with a heavy disc merged at one end,
  dropped sideways, ends with its heavy end down within 10 degrees);
  `a_spinning_disc_slows_in_still_air` (gravity off, the spin decays,
  monotone, and faster at higher density); `drag_still_off_by_default`
  (no torque with both coefficients 0).
- Geometry unit tests: for a still body the panel sum equals
  `WidthProfile::width_across` within 1e-4 at 64 directions over random
  convex shapes (a proptest beside 6-2's); a rectangle and a regular
  polygon about their centre give zero torque at every flow direction.
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-geometry`,
  `-p gradiance-physics`, `--test it air_drag`.

## Out of scope

- Lift, flutter, stall, per-body drag coefficients, shielding of one body
  by another, concave pockets (the hull is used).
- Buoyancy and the fluid drag (Q-5 owns fluids).

## Outcome

Done (2026-11-21).

- `geometry::profile::DragPanels` (≤ `MAX_PANELS` = 64 convex-hull edges:
  midpoint, outward normal, length; a longer hull keeps an evenly spaced
  vertex subset). `facing_width` equals `WidthProfile::width_across` at
  every sample direction (unit test, plus a proptest beside 6-2's).
- `physics::air`: `AirPanels` **replaces** `AirProfile` (nothing else
  read it; `WidthProfile` stays in geometry for the width law and its
  tests). `DragLaw::load(panels, AirMotion, dt) -> (force, torque)` is
  applied with `apply_force` + `apply_torque`. The force is capped as
  before. A torque that *opposes* the spin is capped at `I|ω|/dt`; a
  turning torque is not capped (with `ω = 0` the brief's cap would forbid
  any weathervane torque).
- **Deviation from the brief's quadrature:** each edge is evaluated at
  **two** half-edge midpoints, not one. At an edge midpoint a spinning
  round body only sees tangential air, so the one-point law gives a disc
  zero angular drag and `a_spinning_disc_slows_in_still_air` could not
  pass. For a still body both halves see the same air, so the force is
  unchanged (unit test against `force` at 64 angles).
- Tests: the seven N-4 tests pass (the profile test now reads
  `AirPanels::facing_width`). New: `a_dart_turns_nose_first`,
  `a_spinning_disc_slows_in_still_air`, `drag_still_off_by_default`, and
  unit tests for zero torque on symmetric bodies, the nose-down torque sign
  and the spin cap.
- **Learned:** the dart (2 m shaft, 0.25 m disc at the nose) settles
  **12.4°** off vertical and stays there, not 10°. Pressure-only panels
  (no lift, no skin friction) balance the disc's leading arc against the
  shaft's side at that angle. The test asserts < 15° with that note.
