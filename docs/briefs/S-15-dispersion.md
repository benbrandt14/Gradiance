# Dispersion and white light
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("white light", "rainbow") · status: ready (after S-3)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

A laser marked white splits at the first refraction into a few coloured
rays, each with its own index (Cauchy law). A prism makes a spectrum.

## Read first

- Brief S-3 and its Outcome — `Optics.index`, the refract step in `geometry::beam`.
- `crates/gradiance-physics/src/laser.rs` — the beam budget.

## Facts

- Cost: k colours multiply ray casts by k after the first split. Cap
  k = 7 and keep the existing bounce cap per ray.

## Approach

- `Optics { index, dispersion }` (Cauchy B term); `Laser.white: bool`.
- `geometry::beam` forks a white ray into k coloured rays at its first
  refraction; drawn in their colours.

## Out of scope

Intensity, partial reflection, diffraction.

## Done when

- Pure test: red and violet leave a 60° prism of n = 1.5, B = 0.004 µm²
  at the computed angles ± 0.05°.
- Gates green.

## Outcome

Done 2026-11-23 (PR on the S-14 branch).

- `Optics { index, dispersion }`: `dispersion` is Cauchy's B in µm²
  (serde default 0, held to 0..0.05). `index` holds at the sodium D line
  (0.5893 µm); `index_at(λ)` gives the rest. A new transparent body starts
  with crown glass, B = 0.004 (decision `glass-dispersion-default`).
  `set-index` keeps a body's dispersion. New verb `set-dispersion`
  (`body.set_dispersion`); a mirror body becomes glass. The inspector
  shows a "dispersion" field under the index.
- `Laser.white` (serde default false, skipped when false): the laser
  editor's "white light" box. No verb: node fields have no edit verbs yet.
- `physics::laser`: a white laser traces once more per colour of
  `SPECTRUM_WAVELENGTHS` (7, red to violet) through `index_at`. The hit
  and distance stay the reference trace's. `LaserBeam.split` counts the
  corners every colour shares. The tracer, not a fork in `geometry::beam`,
  does the split (decision `white-laser-full-traces`).
- Render: the shared corners draw in `theme::WHITE_LIGHT`, each tail in
  its `theme::SPECTRUM` colour.
- Tests: the pure prism test is
  `domain::optics::tests::red_and_violet_leave_a_prism_at_the_computed_angles`
  (n = 1.5, B = 0.004, 60°, ± 0.05°, red and violet more than 0.5° apart).
  `tests/it/lasers.rs` `a_white_laser_through_a_prism_makes_a_spectrum`
  checks the app path. The persistence proptest covers `dispersion`.

