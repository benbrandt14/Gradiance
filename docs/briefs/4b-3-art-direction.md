# One look: palette, theme, shading
Phase 4b · owner direction 2026-11-16 · "Take artistic liberty (in visual
instances like SSAO or in color pallette/theme) to ensure consistency and
fluency. If default SSAO is unclear, determine how an artistic direction may
be maintained and provide options." · status: in progress (revised 2026-11-18)

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

*(worker fills in)*
