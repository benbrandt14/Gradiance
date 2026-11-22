# Laser refraction, refractive index and beam colour
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("refractive index", "laser colour") · status: done

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

Done (2026-11-22).

- **Verified first:** avian 0.8's `cast_ray(.., solid: false, ..)` from
  inside a collider reports its boundary (the API docs: "treated as
  hollow"), so the brief held.
- **Pure walk:** `geometry::beam::refract` (Snell; `None` on total
  internal reflection) and `trace_optics`, which tracks the medium the beam
  is in by key. A body with an index bends the beam in; inside, the cast
  asks for that body's exit; at the exit the beam bends out, or reflects
  inside past the critical angle. Every bend counts against the bounce
  limit. `trace_beam` is now `trace_optics` with no refracting bodies, so
  every N-8 test and mirror scene traces the same.
- **Physics:** `queries::cast_beam` takes an optics lookup. Inside a body
  it casts against that body only, with `solid: false`
  (`queries::exit_of`), and steps past the seams of a compound collider (a
  boundary point with the body still on its far side), at most 16 seams.
  A beam inside one body ignores others: no nested media.
- **Authored:** `domain::optics::Optics { index }` (default 1.5, held to
  1..4), optional on a body. It is saved (skipped when absent),
  `PropertyValue::Optics`, kept by the snapshot and inherited by cut
  pieces. The verb `(set-index i n)` (`body.set_index`; 0 or less: a
  mirror again). The inspector has a "transparent (refracts beams)" toggle
  and an index field. `Laser.color: Option<Rgba>` (`None` = the look's
  laser ink, as before) is a palette-swatch row in the laser editor.
- **Polish from the lavapipe look:** a transparent body drew as an opaque
  solid, which read wrong for glass. A body with an index now draws
  translucent at `GLASS_OPACITY` (0.55), keeping its shading.
- Tests: Snell 30° into n = 1.5 gives 19.47° ± 0.01°; 50° inside n = 1.5
  reflects totally; a glass slab lets the beam out parallel and shifted
  (pure); `lasers::a_prism_deviates_a_beam_by_the_minimum_deviation_angle`
  (60° prism, n = 1.5: 37.18° ± 0.5°, parallel to the base inside; a mirror
  again at index 0).
- Refactor in passing: `PropertyValue::write`'s optional-component arms
  share one helper (`set_optional`); the function had passed clippy's
  length limit.

