# Laser refraction, refractive index and beam colour
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("refractive index", "laser colour") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A body has an optional refractive index. A laser beam that hits it bends
(Snell), splits nothing, and reflects on total internal reflection. Bodies
without an index reflect as today. A laser has a colour. A glass prism
bends a beam by the textbook angle.

## Read first

- `crates/gradiance-geometry/src/beam.rs` — the pure reflection walk.
- `crates/gradiance-physics/src/laser.rs` — `trace_lasers`.
- `crates/gradiance-domain/src/laser.rs:50` — the `Laser` fields.
- Brief N-8 Outcome — the budget (`O(lasers × (bounces + 1))` casts).

## Facts

- Every solid body reflects today (N-8). Index ∞ = that behaviour, so the
  default keeps old scenes the same.
- A ray inside a body needs a cast that starts inside a collider. avian's
  ray cast has a `solid` flag; with `solid: false` it reports the exit
  face. *Unverified on avian 0.8*: the worker checks first and returns the
  brief if it does not hold.
- Phase Q owns "laser canvas block". Do not touch that behaviour.

## Approach

- Authored `Optics { index: f32 }` (optional; absent = mirror) through the
  `PropertyValue` path; a material row in the inspector; `(set-index i n)`.
- `geometry::beam`: per segment, pick reflect or refract from the two
  indices and the normal (pure, unit-tested: Snell, critical angle).
  The bounce cap counts refractions too.
- `Laser.color: Rgba`, inspector colour picker, beam drawn in it.

## Out of scope

Dispersion, white light, partial reflection, lenses as a tool.

## Done when

- Pure tests: a 30° ray into n = 1.5 leaves at 19.47° ± 0.01°; a ray at
  50° inside n = 1.5 reflects totally.
- Behaviour test: a beam through a 60° prism of n = 1.5 deviates by the
  computed minimum-deviation angle ± 0.5°.
- fmt, clippy, tests, doc gate green.

## Outcome
