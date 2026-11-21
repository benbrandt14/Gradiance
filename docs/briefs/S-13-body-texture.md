# Image texture on a body
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("texture tool") · status: ready · feel: Illustrator (drawing) (`docs/interaction-feel.md`)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

A body can show an image on its front face, placed by an offset, scale and
angle in the body's frame. The image goes through the toon ramp (or a
"raw" switch) so it fits the look. The texture tool drags the placement.

## Read first

- `crates/gradiance-render/src/toon.rs`, `toon.wesl` — the front-face material.
- `crates/gradiance-domain/src/appearance.rs:159` — `Appearance`.
- `docs/ui-design.md` — tool and inspector rules.
- Memory note: the owner's toon direction (2026-11-19).

## Facts

- The save format is single-file RON. An image is an asset path relative
  to the scene file, not embedded. *Decide* whether to copy the image next
  to the scene on save; record it in `archive.ron`.
- UVs: the extrusion mesh has no UVs for the front cap today (*unverified*;
  check `extrude_sync.rs`).

## Approach

- Authored `Texture { path, offset, scale, angle, mode: Toon | Raw }`,
  optional, `PropertyValue::Texture`, `(set-texture i path)`.
- Front-cap UVs from the body frame; the shader samples and tints.
- Texture tool: drag = move, wheel = scale, right-drag = rotate. One
  undo step per gesture.

## Out of scope

Side-face textures. Animated images. Shape from image (S-14).

## Done when

- Snapshot test of a textured box on lavapipe; save/load round trip.
- Gates green; `FORMAT_VERSION` bumped if the record changes shape.

## Outcome
