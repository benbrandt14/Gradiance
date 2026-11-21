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
