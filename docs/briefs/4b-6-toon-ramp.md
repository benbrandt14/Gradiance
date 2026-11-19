# Toon ramp: themed gradient, matte, dithered bands
Phase 4b · owner direction 2026-11-19 (thread message with a reference
image, `docs/briefs/assets/4b-6-reference.gif`) · status: in review
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

### Landed

- **`domain::ramp`**: `ColorRamp` (stops: position + tone; `Constant` or
  `Linear`), `eval` (authoring form), `table()` (sampled once into 128
  tones; the shader indexes, never searches), `insert` / `remove` /
  `move_stop` (kept sorted, never empty, at most 16 stops), and `shade` /
  `hard_light`: a tone tints a fill by **hard light** in gamma space (mid
  grey 0.5 = the fill as authored, darker multiplies, lighter screens), so
  every body goes through the same falloff.
- **`RenderSettings`** (scene content: saved, undoable): `ramp`, `dither`
  (edge zone width in lighting levels), `dither_scale` (pixels per dither
  cell), `bevel` (new); `relief` now scales the face-field tilt; `bands`
  removed (the stops are the bands). Serde defaults, so old files load;
  `FORMAT_VERSION` stays 8.
- **Looks** (`theme::Look::ramp`, `render`): studio ships a 5-stop ramp
  (cool mauve-brown deep shadow, mauve shadow, near-neutral mid, the fill
  as authored, a warm cream lit patch), white point 1.25 (a face lit by
  both lights lands in the authored band, a face turned to the key rises
  into the cream patch), dither 0.025 in 2-pixel cells, relief 1.0, bevel
  0.6, **specular 0** (matte). Blueprint (3 cool stops) and poster (2
  stops) keep their old band edges, flat faces, no dither. No look has a
  glint; `specular` stays as a setting.
- **Shader** (`toon.wesl`, `plane.wesl`): the lit/albedo level, offset by a
  4x4 Bayer threshold times `dither`, indexes the ramp table; the tone tints
  the fill. Rim and emissive as before. The back plane and grounds use the
  same ramp and dither, so shadows take the bodies' hues.
- **Normals from the geometry**: `geometry::face_field` samples, per body,
  the distance and direction to the polygonized outline over the face
  frame (64 texels on the long side; holes and cuts included); `render::extrude_sync`
  bakes it with the mesh into a linear `Rgba8Unorm` image
  (`toon::FaceFieldImage`), `toon::attach_face_fields` hands it to the
  material. The shader tilts the front face's normal toward the nearest
  outline by a quarter-round profile: `relief` at the outline, nothing at
  `bevel` of the deepest depth. A circle reads as a dome, a box as a slab
  with lit, chamfered edges; the 4b-5 dome is the fallback without a field.
- **Ramp editor** (`ui::ramp_editor`, Settings > Rendering > Ramp):
  Constant / Linear, add (+), remove (minus glyph), reset to the look's
  ramp, a tone bar with three palette fills previewed through it, stop
  markers (drag to move, click to select, double-click the bar to add,
  right-click to remove), the selected stop's position (`config_field`)
  and tone (sRGB picker). Config seam: a settled edit is one undo step.
- **Operations**: `render.ramp` / `(render-ramp interp stops)` replaces the
  ramp (flat `position r g b` groups); `render.set` reaches one stop by
  reflect-path (`ramp.stops[1].position`, `ramp.stops[0].color.r`).
- **Review**: question `toon-ramp` (Ramp + dither / Ramp + pixel grain /
  Ramp, hard bands) over the look scene and the 4b-5 polygons, with the
  reference image path in its description; `expected_bodies` row 15 each.
  `toon-highlights` withdrawn into `archive.ron` (`NoneOfThese` with the
  owner's chat direction, `ingested_as` this brief); its row is gone.
- **Tests**: ramp (7, domain), face field (3, geometry), toon params and
  ramp clamps (render), the editor's drag, double-click add and right-click
  remove (`egui_kittest`, ui), `look::the_ramp_is_replaced_in_one_undo_step`,
  `look::a_ramp_drag_over_many_frames_is_one_undo_step`,
  `scripting::a_script_replaces_and_edits_the_toon_ramp`.

### Measured (lavapipe, 1280x720, studio, close-up of a 0.75 m circle and a 1.4 m box)

- Bevel 0.45 left the circle a flat puck; 0.7 turned the box into a
  pyramid with a small flat top; 0.6 is the default.
- Dither 0.04 stippled most of the circle (its level changes slowly near
  the middle); 0.025 keeps the grain to the band edges.
- The first shadow tones (blue-heavy) made the fully shadowed back plane
  under the ground line a saturated purple; the shipped tones are greyer
  but still shift toward mauve. Darker mid and shadow tones (0.42 / 0.34)
  gave the terminator the contrast the reference has.
- No `failed to create shader module` in the app log.
- Screenshots (not committed): the side-by-side with the reference is in
  the PR description's evidence list.

### Decisions

`decision: true` entries in `docs/review/archive.ron`: `toon-ramp-blend`
(hard light), `toon-face-field` (a baked per-body texture, not interior cap
vertices or a shader SDF), `toon-ramp-settings` (`bands` removed, glint off
in every look, `relief` rescaled, dither and relief studio-only). In scope:
the archived `look` question's blueprint and poster options now pin their
ramp and `dither: 0.0` (their `bands` fields dropped), so
`the_look_question_shows_the_coded_presets` still holds.

### Not checked

- `cargo test --workspace` (CI runs it).
- A real GPU (all screenshots are lavapipe); a high-DPI screen (the dither
  cell is in physical pixels).
- The `toon-ramp` question in the interactive `--review` app by a person
  (the guard test loads every option).
- Blueprint and poster with relief or dither turned on.

### For later

- The outline width: the reference has a thick ink outline; it is out of
  scope here (one setting, if the owner asks for it in `toon-ramp`).
- A pixelation of the lighting term itself (the brief's optional item):
  only the dither cells scale; the lighting is per pixel.
- The face field is rebaked on every shape change (64 x 64 distance
  queries against the outline): cheap for hand-made scenes, worth
  measuring for a script that spawns hundreds of CSG bodies.

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --
-D warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `cargo test -p` domain, geometry,
render, scene, `-p gradiance-ui --lib`, and the `it` modules review, look,
persistence, replay, snapshot, ui_snapshots (`GRADIANCE_REQUIRE_GPU=1`),
op_conformance, registry_validation, scripting: all green. rustc 1.100.0
(pinned).

### Regenerated

`crates/gradiance-scene/golden/v8_every_quantity.ron`, the four
`tests/it/golden/replay_*.ron` (the ramp and the new fields), `docs/ops.md`
(`render.ramp`). No UI snapshot changed.
