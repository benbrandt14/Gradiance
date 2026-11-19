# Toon ramp: themed gradient, matte, dithered bands
Phase 4b · owner direction 2026-11-19 (thread message with a reference
image, `docs/briefs/assets/4b-6-reference.gif`) · status: ready
(after U-9; stacked on #272)

Written by the orchestrator, 2026-11-19. Supersedes the open review
question `toon-highlights` (4b-5): the owner answered its direction in
chat with a reference image, so the question is withdrawn, not asked.

## Owner direction (verbatim)

"I'm thinking something more like this for the toon shader. Taking more
advantage of the geometry that's present, and having a more themed
gradient picker for the fall off (and so that objects don't look so
dissimilar). It shouldn't look quite as much like a shiny toy, more matte
with tasteful dithering or pixelation."

## What the reference shows (orchestrator's reading)

A sphere with three to four flat tones: a pale lit core, a lighter rim
band toward the light, a mid tone, and a shadow tone that **shifts hue**
(warm to a cool brown/mauve) rather than only darkening; a soft round
highlight that reads as a lit patch, not a glossy pin; a thick dark
outline. The tool beside it is a **textured brush**: a screen-space
texture (tiling 15 × 15) whose strength is set per band (shadow, spec,
rim), so band edges break up into a pattern.

## Goal

1. **Ramp, not steps of one colour.** Lighting (N·L after the lights are
   summed, 0..1) indexes a **gradient ramp**: an ordered list of colour
   stops with a position each. The body's palette colour tints the ramp
   (multiply or a hue-preserving blend) so every body uses the **same
   falloff** and objects read as one family. Each look ships its ramp
   preset (studio warm, blueprint cool, poster two-stop).
2. **Gradient picker.** Settings ▸ Rendering gets a ramp editor (add,
   move, delete stops; pick a stop's colour; constant vs linear
   interpolation, as Blender's ColorRamp). The ramp is scene content
   (`RenderSettings`, undoable through `commit_settings_edits`, saved).
   Sample it once to a small texture or uniform array (two-tier rule:
   the frame loop does a lookup, no search; `kernel::Lut` is the model).
3. **Matte.** The default has no glossy glint. A highlight, if any, is a
   ramp stop (a lit patch), not a specular pin. Keep `specular` only if a
   look wants it; studio's default becomes 0.
4. **Tasteful dithering or pixelation.** Band edges dither with an
   ordered (Bayer 4×4 or 8×8) or blue-noise pattern in screen space, with
   a strength and a pixel scale, so the transition between two stops is a
   pattern, not a hard line and not a smooth blend. Off, the bands stay
   hard. Optional: a pixelation scale for the lighting term only (outline
   and silhouette stay crisp).
5. **Use the geometry.** The light falloff must come from real shape
   normals where they exist: the extruded side walls, and a pillowed
   front face derived from the shape's outline (the SDF distance to the
   contour gives a bevel/pillow normal; the 4b-5 dome over face
   coordinates is the fallback). A circle should read as a rounded body,
   a box as a slab with lit edges. `geometry::polygonize` stays the only
   contouring point.

## Read first

- `docs/briefs/4b-3-art-direction.md` and `docs/briefs/4b-5-toon-highlights.md`
  Outcomes (looks, lights, face coordinates, relief, the glint).
- `crates/gradiance-render/src/toon.rs`, `toon.wesl`, `plane.wesl`;
  `crates/gradiance-domain/src/settings.rs` (`RenderSettings`),
  `theme.rs` (`Look`); `scene::records` `scene_content!` (a new field is
  classified there).
- `docs/ui-design.md` (a new widget: the ramp editor; glyphs from
  `ui::fonts::glyph`; theme tokens).
- `docs/bevy20-notes.md`; shaders fail at run time: check the log for
  `failed to create shader module`.

## Approach

1. Screenshot the current studio look (4b-5 default) as the baseline.
2. `ColorRamp` type in `gradiance-domain` (stops, interpolation), with
   `Look` presets; serde default so old files load (or bump
   `FORMAT_VERSION`, which the owner allows); sample to a 1D texture or a
   uniform array in render.
3. Shader: ramp lookup by lighting; palette tint; dither at stop edges
   (strength, scale); matte default. The back plane and ground use the
   same ramp so shadows match.
4. Pillow/bevel normals for the front face from the outline (pick the
   cheapest that reads well; record the choice).
5. Ramp editor widget in Settings ▸ Rendering, plus an op
   (`render.set` already writes `RenderSettings`; check the ramp
   round-trips through it and the Lisp verb).
6. Review question `toon-ramp`: 3 options over the look scene (for
   example: ramp + dither, ramp + pixelation, ramp hard bands), with the
   reference image path in the description. Withdraw `toon-highlights`
   from `questions.ron` (archive it with `ingested_as` = this brief and a
   note that the owner answered in chat).

## Out of scope

New looks, outline width changes (unless the reference needs a thicker
default: then one setting), 2D lighting crates, SSAO, the panel theme,
physics shape changes (pillow normals are lighting only).

## Done when

- The default studio look is matte, uses a ramp with a hue-shifted
  shadow, and shows dithered band edges; all bodies share the falloff.
  Screenshot evidence beside the reference in the PR.
- The ramp editor edits the ramp with one undo step per gesture; the ramp
  saves and loads.
- Gates green; snapshot fixtures regenerated where the look changed;
  review guard passes. Outcome filled.

## Outcome

*(worker fills in)*
