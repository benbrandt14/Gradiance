# One look: palette, theme, shading
Phase 4b · owner direction 2026-11-16 · "Take artistic liberty (in visual
instances like SSAO or in color pallette/theme) to ensure consistency and
fluency. If default SSAO is unclear, determine how an artistic direction may
be maintained and provide options." · status: in review (revised 2026-11-18)

## Goal

The app reads as one designed thing. You have **artistic liberty**: choose a
direction and apply it consistently to:
- the body palette (4-3's ten colours);
- the egui theme (panel colours, accents, selection highlight);
- gizmo and glyph colours (snap, joints, grid, guides);
- background and back/ground planes;
- lighting;
- shading: SSAO, contact shadows, borders, the toon/rim settings.

Present **2–3 named directions** as screenshot sets. Implement the one you
recommend as the default. Make the others selectable presets, so the owner
can switch and compare.

SSAO specifically: 4-3 found it nearly invisible at scene scale. Determine:
- whether AO serves the look at all;
- if so, which radius/intensity at metre scale makes it read;
- if not, what replaces it (contact shadows, border weight, a baked
  ground shadow).
Show the options side by side.

## Read first

- `docs/ui-design.md` (the design system; update it to the chosen
  direction).
- `crates/gradiance-domain/src/appearance.rs` (`PALETTE`), `settings.rs`
  (`RenderSettings`, `LightingSettings`, `ScenerySettings`).
- `crates/gradiance-render/src/` (materials, scenery, decorations, gizmo
  colours) and `crates/gradiance-ui/src/fonts.rs` / theme setup.
- `docs/roadmap.md` § V (V1–V5: what each visual setting is for), briefs 4-3
  and 4-4 Outcomes (SSAO/MSAA, the sub-view workaround).

## Facts (orchestrator, 2026-11-16)

- Format and UI may change freely (owner). A preset can be a new
  scene-content field, or a workstation choice. Classify it in
  `EnvironmentRecord` (invariant 4) and say why.
- lavapipe renders the real pipeline, slowly. Screenshots are the main
  evidence, so take them at the same framing for every direction.
- **Testable consistency** (owner: "creatively add additional testing"). At
  least:
  - **contrast:** each palette fill vs the background, and border vs fill,
    meets a minimum (e.g. WCAG-style luminance ratio ≥ 3:1 for shapes);
  - **distinctness:** palette entries are pairwise separated in a
    perceptual space (e.g. OKLab ΔE above a floor), so "select every red
    body" is unambiguous;
  - **one source:** no hard-coded colour literals outside the theme/palette
    modules. A source scanner test, like the glyph rule.

## Owner input (review mode, ingested 2026-11-17)

Question `shading` (`docs/review/archive.ron`): the owner picked **Toon
(today)** over Smooth and Toon + SSAO. Note, verbatim: "A future revamp of
the visuals will be required. The initial build-out was not able to take
screenshots. Some other aspects of 2D lighting ( like the 2d lighting bevy
crate) would probably be good too. The toon banding has never really been
observed because of the flat lighting."

For this brief that means:
- build the presets on toon; SSAO is not the default;
- the default lighting is too flat to show the bands. Change the default
  light direction or band thresholds so that a default scene shows them, and
  put the before and after in review mode;
- a full visuals revamp and a 2D-lighting crate are **not** this brief
  (`docs/state.md`, retained, UI/render).

## Revision (orchestrator, 2026-11-18, on `b53b019`)

The brief predates `docs/unified-surface-plan.md`. Three changes:

- **Scope cut: the egui chrome theme moves to U-7.** Plan § 7.1 puts
  semantic chrome tokens (`ui::theme` driving `egui::Visuals`,
  Blender-leaning) in U-7, and § 10 row 9 pairs U-7 with this brief. This
  brief owns the **scene** look: body palette, gizmo/glyph/grid/guide
  colours, background and planes, lighting, toon bands, SSAO, borders.
  It may set the one **accent** colour and the selection highlight as
  scene tokens that U-7 then reads; it does not restyle panels.
- **Tooling.** Screenshots go through `tools/screenshot/session.sh`
  (U-3/U-4: `ui.screenshot`, `ui.click`, `ui.key`) or the CLI from U-6
  (`gradiance op ui.screenshot --path …`). Set the scene with a
  `--script` file so every direction uses the same framing.
- **Owner questions.** The before/after of the toon-band lighting and
  the direction choice are interactive: add them to
  `docs/review/questions.ron` with scenes (review mode), do not ask in
  chat. Implement the recommended direction as the default meanwhile.
  Non-interactive choices (where a preset lives, test thresholds) are the
  worker's, recorded in the Outcome.

## Approach

1. Survey: screenshot the current look across a standard scene: stacked
   boxes, a joint chain, a glowing body, a selection, snap glyphs, the
   panels open.
2. Define 2–3 directions (e.g. "clay studio", "blueprint", "flat graphic"):
   palette + theme + shading per direction, as data.
3. Implement presets and choose the default. Move scattered colour literals
   into the theme.
4. Screenshot every direction at the same framings; SSAO options side by
   side.
5. Consistency tests (above).

## Out of scope

New render features beyond tuning existing ones, custom fonts beyond what
exists, the group workflow (4b-1/4b-2), `docs/state.md`.

## Done when

- Presets selectable; the default chosen and justified in the Outcome.
- Screenshot sets per direction plus the SSAO comparison, described in the
  PR (not in git). The orchestrator shows them to the owner.
- Consistency tests; `docs/ui-design.md` updated. Gates green. Outcome
  filled.

## Outcome

### Landed

- **`gradiance_domain::theme`**: `Look` (`studio` default, `blueprint`,
  `poster`), one `SceneTheme` per look (palette, neutral, ground, rod,
  border, backdrop, overlay `Ink`), each look's render/lighting/scenery
  preset, the `Role`/`Stroke` ink vocabulary, the `debug` overlay colours,
  and the colour math (WCAG luminance and contrast, OKLab, ΔE) with the
  consistency floors (`theme::floors`). `PALETTE` is now the studio palette
  (OKLCH L 0.585, ten hues).
- **Semantic palette:** `RenderSettings::look` (scene content, one undo
  step). The render layer shows a palette slot, the default border, neutral,
  ground and rod in the active look's tone (`SceneTheme::display`); custom
  colours show as authored. A look change rebuilds body and ground
  materials.
- **Every scene colour moved into the theme:** grid, joints, borders,
  radius lines, tracers, node glyphs, snap glyphs, guides, selection,
  handles, every tool preview (`ToolPreview` now holds roles), pack ghost,
  debug overlays, back plane and clear colour, light tints. The scripted
  ground now gets the ground tone (it took a random palette colour).
- **Operations:** `look.apply`, `look.list`, `render.set`, `lighting.set`,
  `scenery.set` (config seam; `OpOutput::replace_setting` writes a whole
  settings resource). Body property `emissive` (for `group-set` etc.).
- **UI:** Settings ▸ Rendering ▸ Look picker (applies the preset); the
  "add light" button adds a fill-strength light (its 6000 lux was over the
  white point).
- **Toon bands visible (owner note in `shading`).** Three causes, all fixed:
  1. the front cap of every extruded body was wound away from its normal;
     the double-sided material then flipped the normal, so the face toward
     the camera was lit as facing away (ambient + rim only).
     `geometry::extrusion` now orients every triangle to its normal (test
     `every_triangle_winds_toward_its_normal`);
  2. the 12 000 lux key put lit faces far past the white point (one band);
     the studio preset uses a warm key (3000 lux) and a cool fill (1250 lux)
     with ambient 330, measured on lavapipe so that lit-by-both, fill
     shadow, key shadow and both shadows land in bands 4, 3, 2, 1;
  3. the back plane and grounds were not banded: `plane.wesl` now applies
     the same quantization (uniform kept in step by
     `toon::apply_render_settings`), so shadows are flat steps.
- **Colour fidelity:** the scene camera uses `Tonemapping::Linear`
  (TonyMcMapface darkened saturated fills by up to a third). Emissive glow
  was invisible (Bevy multiplies alpha-1 emissive by the exposure, about
  1/1000): it is now unexposed and added after the bands.
- **Tests:** theme unit tests (contrast, distinctness, display tone, colour
  math, presets), `tests/boundaries.rs::scene_colours_have_one_source` (+ a
  scanner self-test), `tests/it/look.rs` (apply = one undo step, ops, errors),
  `tests/it/review.rs::the_look_question_shows_the_coded_presets`.
- **Review questions** (`docs/review/questions.ron`, scenes in
  `docs/review/scenes/look/`): `look` (which direction is the default) and
  `toon-lighting` (old light values vs the new default vs tilted).

### Decisions (worker, non-interactive)

- **Default: Clay studio.** Only it uses two shadowing lights, which is what
  makes all four bands read in the plain 2D view; it is closest to the
  toon look the owner picked. The owner's choice is the `look` question.
- **Where a preset lives:** the look is a field of `RenderSettings`
  (scene content: saved with the scene, undoable), not workstation config:
  it changes what the document looks like, like the lights it sets.
  Applying a look writes its presets; the field alone only re-tones colours.
- **Palette slots are semantic** (toned at display) rather than rewritten
  in the save file: switching looks needs no command over every body and
  keeps custom colours exact.
- **Thresholds:** fill/rod vs backdrop, outline vs fill, ink vs backdrop,
  joint face vs outline ≥ 3:1 (WCAG non-text); ground vs backdrop ≥ 1.2
  (a surface whose edge is carried by its trace line and shadows; blueprint's
  navy ground is 1.27); palette pairwise OKLab ΔE ≥ 0.045 (blueprint's pale
  palette is the tightest at 0.046).
- **One-source scope:** `gradiance-ui` is exempt (chrome is U-7's) and test
  modules are exempt.
- **SSAO: not part of any look.** With SSAO on, radius 0.3 m, 0.73 m (Bevy
  default) and 1.5 m gave pixel-identical frames; the only change from SSAO
  off is edge aliasing (it turns MSAA off). In this view AO only scales the
  ambient term, which the bands quantize away, and the scene has no visible
  creases head-on. The cues it was meant for come from the outlines and the
  banded, layered key/fill shadows. Radius/thickness settings were tried
  and removed (no visible effect). SSAO stays a Lighting-tab toggle.

### Measured (lavapipe, 1280x720)

- Studio front faces at the white point 1.0: ambient ≈ 0.2, fill ≈ 0.25,
  key ≈ 0.5 (back-plane pixels: 234 / 206 / 154 / 189 red channel for the
  four regions). Old values: a lit face at ≈ 2.3 × the white point.
- Screenshots: `/mnt/project-files/4b-3/` (README lists them):
  `looks-side-by-side.png`, `lighting-before-after.png`, per look
  `-scene`, `-tilted`, `-selection`, and the SSAO set.

### Not checked

- `cargo test --workspace` (CI runs it). The whole `it` binary was not run
  locally; modules run: look, review, op_conformance, replay, editor_ops,
  persistence, ui_groups, ui_panels, ui_snapshots, tools, rod, scripting,
  registry_validation.
- Real GPU rendering (all screenshots are lavapipe).
- The `look` / `toon-lighting` questions in the running `--review` app
  (the guard test loads every option headless).

### Cut

- The inspector's and Groups panel's swatch rows still draw the studio
  tones under another look (needs the look in those panels; U-7 territory).
- The egui chrome theme (moved to U-7 by the revision).
- Per-look line weight (gizmo width is one global setting).
- A 2D-lighting crate / full visuals revamp (retained in `state.md`).

### Regenerated

- `crates/gradiance-scene/golden/v7_every_quantity.ron` (new defaults,
  `look` field), the four `tests/it/golden/replay_*.ron` (default fill,
  border and environment values only), `tests/snapshots/ui/groups_panel.png`
  (palette chips),
  `docs/ops.md` (5 new operations).

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `-p` tests of domain, geometry, op,
command, render, interaction, scene, and the `it` modules above: all green.
rustc 1.100.0 (pinned).
