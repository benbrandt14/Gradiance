# M19 chrome, functional
Phase 4 · M19 · "Themed default palette; quantized color picker; random
colors within a grouped selection (1.5) · Camera settings section (zoom
sensitivity etc.) (10.2) · Sim-settings UI: scrub-drag values, gravity
direction widget (8.2)" · status: ready

## Goal

Ship the remaining M19 chrome items **functional, not polished**. Per
`docs/orchestration.md` § Phase 4 and § Future intent 1, the UI will be
replaced, so each item is done when:
- its state lives in a lower crate behind a settings resource or intent;
- it is reachable from a script or registered op where the item is
  document/config state;
- its egui surface is the thinnest thing that exercises it.

Also confirm, with screenshots, that M19's **render** items have landed and
mark them in the roadmap: emissive, AO/contact shadows, body borders.

## Read first

- `docs/roadmap.md` § M19 and § V (V1 landed SSAO + contact-shadow toggles).
- `crates/gradiance-domain/src/appearance.rs`: `Appearance { fill, border,
  emissive }`, `Appearance::for_id`, the hue-from-id rule shared by tools and
  verbs.
- `crates/gradiance-ui/src/context_menu.rs` ≈L694: "Random colors (per
  body)", the golden-angle hue walk over a selection, one
  `PropertyEditIntent`.
- `crates/gradiance-interaction/src/camera.rs` ≈L25–35: `ZOOM_STEP`,
  `KEY_PAN_SPEED`, `ORBIT_SPEED`, `GLIDE_RATE` as consts.
- `crates/gradiance-ui/src/settings.rs`. The Simulation tab is a
  `reflect_grid_units` over `SimSettings`, with gravity as a raw `Vec2`.
- CLAUDE.md invariant 4: scene-content vs workstation-config resources. A
  new settings resource **must** be classified in
  `scene::records::EnvironmentRecord::scene_content_eq` /
  `apply_scene_content`.
- `docs/ui-design.md` (glyphs from `ui::fonts::glyph`; `precise_drag` vs
  `labelled_drag`).

## Facts (orchestrator, 2026-11-14, main `294a47d`)

- **Already present, not to be rebuilt:**
  - per-body border colour/alpha and emissive, in the inspector's
    Appearance section and the context menu;
  - `render::decorations::draw_body_borders`;
  - emissive in `material_sync`;
  - SSAO/contact-shadow toggles (V1);
  - random colours over a selection.
  *Unverified:* whether the borders, emissive and AO render visibly in the
  current build. The screenshot pass settles it; a defect found there is in
  scope.
- "Random colors within a **grouped** selection" vs today's "per body":
  read it as one colour per group (bodies sharing an outermost
  `SelectionGroup` get the same hue). Unverified whether that is the
  intended reading. If you take it, keep "per body" too.
- No camera settings resource exists. Camera feel belongs to the person,
  not the document, so it is **workstation config**, like `GridSettings`.
  It must never enter undo.
- "Themed default palette; quantized color picker": a small fixed palette
  (e.g. 8–12 hues), used:
  - by `Appearance::for_id` for new bodies, instead of any hue;
  - by a swatch row in the Appearance section, instead of a free picker.
  The free picker can stay behind a disclosure. The palette is data in
  `gradiance-domain`, so a new shell reads the same table.
- Gravity-direction widget: a small dial/drag that edits `SimSettings.gravity`
  as angle + magnitude. It goes through the settings seam, which
  `commit_settings_edits` turns into one undo step per settled edit.

## Approach

1. Palette:
   - the palette table in domain;
   - `for_id` picks from it, deterministic per id;
   - Appearance swatches;
   - a verb only if trivially fits; otherwise record `set-color` as still
     retained (it is on the retained list).
2. Grouped random colours as above, one `PropertyEditIntent`.
3. `CameraSettings` workstation-config resource (zoom step, pan speed, orbit
   speed): classified in `EnvironmentRecord`, read by `camera.rs`, edited in
   a "Camera" settings section.
4. Gravity direction + magnitude widget on the Simulation tab. "Scrub-drag
   values" means the Simulation scalars use the drag widgets the design
   system names. Swap them in if the reflect grid doesn't already.
5. Screenshots of each, plus a body with a border, an emissive body and AO
   contact shading. Mark landed items in the roadmap.

## Out of scope

Gradient pickers and colour-by-signal (deferred in § V), theming egui itself,
new panels beyond the Camera section, the retained `set-color` verb unless it
falls out trivially (say which), `docs/state.md`.

## Done when

- Tests:
  - `for_id` lands on the palette;
  - grouped random colours share a hue per group, with one undo;
  - `CameraSettings` round-trips through save, is absent from undo, and
    `scene_content_eq` ignores it;
  - a gravity widget edit is one undo step (or the underlying angle/magnitude
    ↔ `Vec2` helper is unit-tested, if the widget isn't testable headless:
    say which).
- Screenshots via `tools/screenshot/` of:
  - the palette swatches;
  - the Camera section;
  - the gravity widget;
  - a scene showing borders, emissive and AO.
  Describe them in the PR, keep them out of git, and report the paths.
- Roadmap M19 lines updated. Gates green. Outcome filled.

## Outcome

*(worker fills in)*
