# Material presets and the material for new bodies
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("material presets", "material for new objects", "mass entry") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A short table of named materials (for example wood, steel, rubber, ice,
glass, foam) sets density, friction, restitution, colour and, after S-3,
refractive index in one undo step. New bodies take the current default
material. The inspector also accepts a mass and writes the density.

## Read first

- `crates/gradiance-domain/src/tuning.rs:102` — friction scale notes.
- `crates/gradiance-domain/src/appearance.rs:196` — the colour-preset table
  shape to copy.
- `crates/gradiance-domain/src/settings.rs:334` — `ToolDefaults`, where the
  default material lives (workstation config, not undo).
- `crates/gradiance-command/src/property.rs:17` — `PropertyValue`.

## Facts

- Density is areal (kg/m²) by the units decision. Preset values must be
  areal, chosen so that "wood" floats on the N-7 default fluid (2 kg/m²)
  and "steel" sinks.
- No preset table exists (grep for `preset|wood|rubber`, 2026-11-20).

## Approach

- `domain::material::MATERIALS: &[Material]`, one data table.
- `body.set_material` (`(set-material i name)`): one `property-edit` step
  over the existing fields. No new mutation path.
- Context menu Material row + inspector picker; a "default material" picker
  in Settings > Tools writes `ToolDefaults.material`.
- Mass field: shows `density × area`, an edit writes `density = m / area`.

## Out of scope

User-defined material files. Per-material sounds or textures.

## Done when

- Test: every preset applied to a box gives its table values; undo restores.
- Behaviour test: wood floats, steel sinks in a default fluid.
- fmt, clippy, tests, doc gate green; `docs/ops.md` regenerated.

## Outcome
