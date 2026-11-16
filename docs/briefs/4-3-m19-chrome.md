# M19 chrome, functional
Phase 4 · M19 · "Themed default palette; quantized color picker; random
colors within a grouped selection (1.5) · Camera settings section (zoom
sensitivity etc.) (10.2) · Sim-settings UI: scrub-drag values, gravity
direction widget (8.2)" · status: done

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

Status: done (worker, 2026-11-16). All five M19 lines marked landed in
`docs/roadmap.md`.

**Palette (1.5).** `domain::appearance::PALETTE` is ten named `Swatch`es
(`hsl(h, 0.65, 0.55)`, names are a stable API: red … pink).
`Appearance::for_id` now picks `PALETTE[palette_index(id)]` (id mod 10), so
tools and `spawn-*` verbs land on it. The inspector's Appearance section is a
swatch row (click = set every selected body's fill RGB, keeping alpha,
border and emissive; one `PropertyEditIntent`); the free fill picker sits
behind a "custom fill" disclosure; border and emissive are unchanged.

**Grouped random colours — reading taken.** "Random colors within a grouped
selection" = one colour per group: bodies sharing an *outermost*
`SelectionGroup` id get one palette colour, an ungrouped body counts as its
own group. Unverified against the original feedback; the context menu keeps
"per body" next to the new "per group". Both are a thin wrapper over
`appearance::random_fills(members, grouped)` (pure, in domain): distinct keys
get distinct palette slots, starting at the first body's slot with stride 3
(coprime with 10, so up to ten groups all differ). One intent, one undo.

**`set-color`: still retained.** It does not fall out trivially — the script
snapshot (`SceneView`/`BodyView`) carries no `Appearance`, so a reversible
verb needs the snapshot extended first. The context-menu buttons stay
hand-wired.

**`CameraSettings` (10.2).** New `domain::settings::CameraSettings`
(`zoom_step` 0.94, `pan_speed` 600 screen px/s, `orbit_speed_deg` 0.3), read
by `interaction::camera` through `sanitized()` (zoom step clamped to
[0.5, 0.995], speeds ≥ 0, non-finite → default). **Classified as workstation
config** in `EnvironmentRecord`: a `camera` field (saved, `#[serde(default)]`
so pre-M19 files load), deliberately absent from `scene_content_eq` /
`apply_scene_content`, so undo never touches it; the doc comment there names
it. Edited on a new **Camera** settings tab (`labelled_drag`, config seam;
the orbit/zoom hint moved there from Debug) and reachable from scripts as
`camera-get` / `camera-set` (a generic `register_config_pair` now backs both
`sim-*` and `camera-*`; `ConfigWrite` carries a target). Orbit was
0.005 rad/px (≈0.286°); the default is now 0.3°/px (5 % faster).
*Defect fixed on the way:* keyboard pan was `6.0 * dt * world_per_pixel`,
i.e. 6 screen px/s — the SI flip divided the original `600.0` by 100 although
it was already multiplied by the world-per-pixel scale (the same miss as
`PERSPECTIVE_NEAR`). The default is 600 px/s again. Not screenshot-verified
(keyboard input does not reach the app under Xvfb); the arithmetic and the
`1d013bb` history are the evidence.

**Gravity widget + scrub-drag (8.2).** `SimSettings::gravity_polar()` /
`gravity_from_polar(angle_deg, magnitude)` (domain, unit-tested: round trip,
zero gravity reports −90°, negative magnitude → zero). The Simulation tab is
hand-drawn: a click/drag dial (arrow = gravity direction; grey when
weightless) plus `labelled_drag` rows for direction (°) and strength (m/s²)
and for speed, substeps, timestep, plane friction; any future `SimSettings`
field still appears via `reflect_grid_except(SIM_HAND_DRAWN)`. It writes the
`gravity` vector through the settings seam. **Test choice:** the egui dial is
not driven headless; `history::a_gravity_direction_drag_is_one_undo_step`
pins the seam it writes through (eleven frames of polar edits → one undo),
and the screenshot pass confirmed a real dial drag shows up as a single
"Undo Settings".

**Render items — screenshot verdict.**
- Borders: render (default dark outline; a red α 0.8 border; α 0 hides it).
- Emissive: renders, subtly — an emissive-3 circle reads clearly lighter
  than its matte twin under toon banding; no bloom (none configured).
- Contact shadows: render (hard drop shadows behind every body).
- **SSAO: was broken — fixed.** Turning SSAO on blanked the entire 3D
  viewport (only egui visible). Cause: Bevy keys a camera's main texture by
  `(target, usage, format, Msaa)`; SSAO sets the scene camera to
  `Msaa::Off`, the egui overlay camera kept 4x MSAA, so the two stopped
  sharing a texture and the overlay's output replaced the scene. Not
  lavapipe-specific. Fix: `render::scenery::match_overlay_msaa` copies the
  scene camera's `Msaa` to every other camera each frame (unit-tested with
  `run_system_once`). After the fix the scene renders with SSAO on, but the
  AO itself is barely distinguishable from off in the straight-on view
  (coplanar front faces; the back plane is 0.2 m behind) and only faintly
  at creases when orbited.

**Tests added.** domain: `for_id_lands_on_the_palette`,
`palette_names_are_unique`, `grouped_random_fills_share_one_color_per_group`,
`per_body_random_fills_ignore_groups`, `gravity_polar_round_trips`,
`camera_settings_sanitize_into_a_usable_range`; script:
`camera_verbs_read_and_write_camera_settings`; render:
`overlay_cameras_follow_the_scene_cameras_msaa`; it:
`editor_ops::grouped_random_colors_share_a_hue_per_group_with_one_undo`,
`history::camera_settings_are_workstation_config` (absent from undo +
`scene_content_eq` ignores it), `history::a_gravity_direction_drag_is_one_undo_step`,
`persistence::camera_settings_round_trip_and_pre_camera_files_parse`,
`registry_validation::every_documented_camera_path_resolves`; the proptest
scene strategy now varies the camera too. The four replay goldens gained the
(default) `camera` block — regenerated, the diff is only those five lines.

**Follow-ups (not done).**
- SSAO leaves a black strip at the bottom of the viewport the height of the
  menu bar (22 px) in the 2D view. Likely Bevy SSAO sampling its
  viewport-sized texture with framebuffer coordinates when the camera has a
  viewport offset (`apply_scene_viewport`). SSAO is off by default.
- SSAO is visually near-inert at this scene scale; tuning
  (`ScreenSpaceAmbientOcclusion` quality/thickness) or dropping the toggle is
  a design call.
- `set-color` (and a `set-palette i name` form) needs `Appearance` in the
  script snapshot.
- The swatch row's "current" highlight only shows when the fill matches a
  palette entry exactly (a custom fill highlights nothing — intended).
- The grouped reading of "random colors" is a guess worth confirming with
  the owner.
