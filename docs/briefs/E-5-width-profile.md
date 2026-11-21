# Width profile: tapered open paths
Phase E · roadmap § "Unified plan" · status: ready (after E-3)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 11). Docs only.

## Goal

An open path's width can vary along it: width points (position by arc
length, width) dragged on the canvas with a Width handle in Direct
Selection; the rod tapers between them (tentacles, tapered beams,
flippers). A uniform width stays the default.

## Read first

- `docs/interaction-feel.md` — row **Illustrator**.
- `docs/briefs/E-1-path-leaf.md` — the open-path field.
- VectorCraft `crates/tools/src/distort/width.rs`, `crates/doc/src/appearance/profile.rs` at `9fff48d`.

## Facts

- A tapered capsule between two radii has an exact SDF ("uneven capsule");
  a polyline of them with radii interpolated by arc length is the field.

## Approach

- Feel: **Illustrator**. `PathDef::width` becomes `Width::Uniform(f32) |
  Width::Profile(Vec<(f32, f32)>)` (normalized arc length, width), sorted,
  ≥ `MIN_SHAPE_SIZE`.
- Width handles: a pair of dots across the path at each width point; drag
  sets width; drag along moves it; double-click adds. One `ReshapeIntent`
  per drag. `path.width` operation.

## Out of scope

Width on closed paths. Width presets.

## Done when

- Pure tests: uneven capsule SDF exact at both ends and the middle; a
  profile with equal widths equals the uniform rod.
- Behaviour test: a tapered rod's mass is the integral of its width
  (± 1 %).
- Gates green; screenshot.

## Outcome
