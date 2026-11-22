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

Done (2026-11-22).

- **Record:** `domain::texture::BodyTexture { path, offset, width, angle,
  mode: Toon | Raw }`, saved as an optional `BodyRecord.texture`. The field
  is additive with a serde default, so `FORMAT_VERSION` stays 8 (as for
  the S-7 and S-8 fields). The name is `BodyTexture`, not `Texture`, so it
  never meets a render type of that name.
- **Path:** the file path only. A relative path resolves against the scene
  file's folder (`core::scene_folder::SceneFolder`, written by persist on
  save and load), else the working directory. Saving does not copy the
  image (decision `texture-path-only`).
- **Operations:** `body.set_texture` (`(set-texture i path)`; a new image
  is centred on the face and as wide as it; `""` removes it),
  `body.place_texture` (`(place-texture i x y width angle)`),
  `body.set_texture_mode` (`(set-texture-mode i "raw")`). One undo step
  each. A cut piece keeps the image where it was.
- **Render:** `render::texture_sync` reads each file once (a cache by
  path), and gives the toon material the image and an affine map from the
  face coordinates the extrusion already writes to image coordinates. The
  shader samples it on the front cap only, inside the image's rectangle.
  Toon: the image is the albedo, so the ramp shades it. Raw: it goes over
  the lit result.
- **Tool:** `ToolState::Texture`, palette "Modify", the `texture` icon.
  Drag moves, Shift-drag scales about the centre, right-drag turns. One
  undo step per gesture. The wheel zooms the canvas in every tool, so the
  scale is a Shift-drag (decision `texture-tool-scale`).
- **Inspector:** an "image" row (path, Enter applies; raw; remove image).
- **Not done:** the brief's lavapipe snapshot test. The project has no 3D
  render snapshot harness (the U-5 snapshots are egui only); building one
  is its own brief (decision `texture-render-check`). I checked the shader
  on lavapipe with `tools/screenshot` instead: a toon box, a raw box and a
  turned image on a circle render as expected.
- **Tests:** `tests/it/texture.rs` (set, place, mode, remove, undo; save
  and load; a cut keeps the image in place), the persistence proptest
  (the field round-trips), unit tests for the affine maps
  (`texture::local_to_image`, `texture_sync::image_rows`) and the tool's
  gesture maths, the undo matrix and the conformance suite.

