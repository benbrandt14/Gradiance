# Theme tokens: Blender-leaning chrome
Phase U · `docs/unified-surface-plan.md` § 7.1 and § 10 row 9 (U-7 pairs
with 4b-3) · stacked on 4b-3 (PR #257) · status: in review

Written by the worker, 2026-11-18 (owner ruling "scoped work": a session
may write the brief when the work is in the plan).

## Goal

The editor chrome draws with **semantic tokens**, not colour literals. A
`ui::theme` module holds one token set per chrome preset — panel
background, header, window, field, widget inner / hovered / outline, text
strong / normal / weak, accent, selection, warning, error — turns it into
`egui::Visuals`, and hands the same set to the panels that paint
themselves (console syntax, plot series, node canvas, view cube, label
plates). The default leans to Blender's default dark theme: neutral greys,
low-contrast chrome, one accent. The accent and the selection highlight
are the scene look's (4b-3), read from `gradiance_domain::theme`, never
copied.

## Read first

- `docs/briefs/4b-3-art-direction.md` (Revision: the chrome moved here;
  Outcome: `Ink::accent`, the one-source scanner, the swatch cut).
- `crates/gradiance-domain/src/theme.rs` (`Look`, `SceneTheme`, `Ink`,
  `contrast_ratio`), `crates/gradiance-ui/src/fonts.rs` (glyph tests stay).
- `docs/ui-design.md` (§ 1 glyphs, § 4 seams, § 6 snapshots, § 7 scene
  look).
- `tests/boundaries.rs` (`scene_colours_have_one_source`),
  `tests/it/ui_snapshots.rs`.

## Facts (worker, 2026-11-18, on `1f1428b`)

- The UI installed **no visuals at all**: every panel ran on egui 0.36's
  `Visuals::dark()` (panel `#1b1b1b`, label text grey 140, buttons grey 60,
  selection `#005c80`). Weak text (140 at alpha 0.6, about grey 95 on 27)
  is 2.7:1 against the panel.
- Chrome colour literals in `gradiance-ui` (counted with the extended
  scanner on `1f1428b`): 43 sites in 13 files — `node_graph.rs` 8 (frames,
  canvas, dots, selection, pins), `settings.rs` 6 (dials), `console.rs` 5
  (syntax, ok/error), `plot.rs` 4 (series), `view_cube.rs` 4, `curve.rs` 3,
  `groups.rs` 3 (white fallbacks), `depth_panel.rs` 2 (+ an HSL layer hue
  that differed from the domain's `debug::layer`), `dock.rs` 2,
  `labels.rs` 2, `review.rs` 2, `signals.rs` 1, `optimizer.rs` 1. The 4b-3
  scanner exempted the whole crate.
- egui_tiles draws the dock tab strip in `extreme_bg_color` in dark mode
  and the active tab's outline in `widgets.active.bg_fill`.
- The inspector's and Groups panel's swatches showed studio tones under
  every look (4b-3 Cut): they had no way to know the look.
- No Blender ran in the container; the Blender values are the shipped
  defaults as recalled (`userdef_default_theme.c`, 4.x), listed in the
  module docs.

## Approach

1. `ui::theme`: `ChromeTheme` (resolved tokens, `visuals()`), presets as
   data, `install(ctx, &theme)` (dark mode + visuals + tokens in the
   context's memory), `tokens(ctx)` for panels, `apply_chrome` (reinstall
   when the preset or the scene look changes).
2. Presets (`gradiance_domain::settings::Chrome`): `blender` (default),
   `blender-blue` (Blender's own selection blue) and `stock` (egui's, the
   before). `ChromeSettings` is workstation config: written directly, never
   undone; `chrome.apply` / `chrome.list` operations; Settings ▸ Interface.
3. Replace every chrome literal with a token; swatches show the look's tone.
4. Tests: contrast (text ≥ 4.5:1 on every surface it sits on, for both
   Blender presets over every look; chart marks ≥ 3:1), selection follows
   the accent, neutral low-contrast greys, visuals carry the tokens. The
   one-source scanner covers `gradiance-ui` (egui constructors and named
   `Color32` constants) with `ui/src/theme.rs` the only exemption.
5. Regenerate the UI snapshot fixtures; before/after screenshots; a review
   question `chrome` (palette and selection hue are interactive choices).

## Out of scope

Widget behaviours (U-8), F3 search / pie menus / status bar (U-9), editor
areas and workspaces (U-10), new panels, keys, fonts (the glyph tests in
`ui::fonts` stay as they are), widget geometry (corner radii, spacing).

## Done when

- No colour literal in `gradiance-ui` outside `theme.rs` (scanner green).
- Contrast tests green; snapshots regenerated; `docs/ui-design.md` has the
  chrome tokens.
- Review question `chrome` with scenes; screenshots in
  `/mnt/project-files/u-7/`.
- fmt, clippy, doc, boundaries, `-p gradiance-ui` and touched `it` modules
  green locally; CI green. Outcome filled; `docs/state.md` updated.

## Outcome

Worker, 2026-11-18, on `1f1428b` (stacked on 4b-3, PR #257).

### Landed

- **`ui::theme`** (`crates/gradiance-ui/src/theme.rs`, new): `ChromeTheme`
  (38 tokens: surfaces, text, accent/selection, report colours and the
  content inks panels paint), `visuals()` → `egui::Visuals`, `install`
  (dark mode, visuals, tokens in the context's memory), `tokens(ctx)`,
  `apply_chrome` (in the egui pass after the fonts; reinstalls when the
  preset or the scene look changes), `color32`/`mix`/`contrast` helpers.
  The panels had no visuals before: egui's stock dark theme.
- **Presets** `gradiance_domain::settings::Chrome` (`blender` default,
  `blender-blue`, `stock` = egui's own visuals and the old panel colours)
  in `ChromeSettings`, initialised in `gradiance-interaction` beside
  `CameraSettings`. Operations `chrome.apply` (`chrome-apply`, config seam,
  no undo) and `chrome.list` (`chromes`). Settings ▸ Interface tab.
- **Every chrome literal replaced by a token** (43 sites, 13 files). The
  menu bar uses `header`; the node canvas, view cube, dials, console, plot,
  curve, depth panel and label plates read tokens; the node graph's
  selected block is outlined in the accent (was a fixed blue).
- **Swatches in the look's tone** (4b-3 Cut): the inspector palette, the
  Groups panel's swatches and colour chips show a palette slot as the
  canvas does (`SceneTheme::display`), through `tokens(ctx).look`.
- **Scanner:** `scene_colours_have_one_source` now covers `gradiance-ui`
  (egui `Color32` constructors with a literal first argument and named
  `Color32::` constants, `TRANSPARENT`/`PLACEHOLDER` excepted); the only
  exemptions are the two theme modules. Self-test extended.
- **Tests:** `ui::theme` unit tests (text ≥ 4.5:1 on every surface for both
  Blender presets × three looks; overlay text over every backdrop; chart
  marks ≥ 3:1; selection keeps the accent's hue; default greys neutral and
  low-contrast; visuals carry the tokens; install/tokens round trip),
  `tests/it/look.rs::the_chrome_preset_is_workstation_config`, the review
  guard over the new question.
- **Review question `chrome`** (`docs/review/questions.ron`, scenes
  `docs/review/scenes/chrome/`): Blender dark + scene accent (default) vs
  Blender dark + blue selection vs stock (before).
- Docs: `docs/ui-design.md` § 8 (tokens, one accent, presets, rules) and
  § 7 (scanner scope), `docs/scripting.md` (chrome verbs), `docs/ops.md`
  regenerated (81 operations).

### Decisions (worker, non-interactive)

- **Blender values:** Blender 4.x default dark theme as recalled (no
  Blender in the container; the module docs list each source):
  region `#303030`, tab strip `#1d1d1d`, text field `#1d1d1d`, button
  inner `#545454`, outline `#3d3d3d`, editor border `#161616`, text
  `#e6e6e6`/`#ffffff`, selection `#4772b3` (blue preset). Adjusted:
  floating windows and menus `#282828` (Blender's box inner; its `#181818`
  menu back made the Settings window read as a hole); hover `#656565`;
  weak text `#9e9e9e` (the dimmest grey at 4.5:1 on `#303030`); Blender's
  report colours keep their hue and are lightened to read as text
  (warning `#f0a030`, error `#ff7070`); the console comment grey-green
  lifted from 4.0:1 to 5.0:1.
- **Accent:** the selection fill is the look's accent mixed toward black
  in 2 % steps until white text holds ≥ 4.6:1 (studio `#cf5604` →
  `#c35104`, blueprint `#feb251` → `#986b31`, poster `#df2414` unchanged);
  hyperlinks mix toward white until ≥ 4.6:1 on the panel. Computed, not
  copied, so a new look needs no chrome change.
- **Preset classification:** workstation config (never undone) and **not
  saved with the scene**, like `ToolDefaults`: a shared scene must not
  restyle your panels. Recorded as `chrome-preset-persistence`
  (`decision: true`) in `docs/review/archive.ron`; it moves to U-10's
  workstation file.
- **Name:** the stock preset is `stock`, not `egui` — `tests/boundaries.rs`
  confines the word `egui` to `gradiance-ui`, and `Chrome` lives in the
  domain (below the scene format, beside `Look`).
- **Depth panel layer lines** now use `theme::debug::layer` (HSL 0.75 /
  0.55) instead of their own HSL 0.6 / 0.5, as their doc comment says.
- **Kept:** the active dock tab's outline is the accent (egui_tiles draws
  it in `widgets.active.bg_fill`), widget geometry (radii, spacing) and
  shadows are egui's.

### Measured

- Contrast on `#303030` (WCAG): text 10.6, strong 13.2, weak 4.9, warning
  6.1, error 4.9, ok 6.5; on `#545454` buttons text 6.1; white on the
  studio selection 4.7. Stock egui: weak text 2.7, label text 5.2.
- Screenshots (lavapipe 1280x720, same frozen scene, before = `1f1428b`):
  `/mnt/project-files/u-7/` — 8 before/after pairs (panels, groups,
  console, settings, View and Edit menus, node graph + plot, depth), the
  Interface tab, the `blender-blue` and `stock` presets, the default chrome
  under Blueprint and Flat graphic (README lists them).

### Not checked

- `cargo test --workspace` and the whole `it` binary (CI runs them).
- Real GPU rendering; the `chrome` question in a running `--review` app
  (the guard loads each option headless).
- Values from a running Blender (recalled, see Decisions).

### Cut

- Widget geometry (corner radii, spacing, shadows) and a Blender font:
  out of scope (fonts stay; U-8 owns widget behaviour).
- The depth panel's footprints show authored colours, not the look's tone
  (the rows are built where the look is not read; a small follow-up).
- A light chrome preset (Blender Light): no need shown yet.
- The `look` scenes' `colour.scm` paints some bodies wrong when its
  indices are read before every spawn lands (seen in the screenshot runs;
  the screenshots paint over remote instead). Not this brief; worth a
  look in the review-mode backlog.

### Regenerated

- `tests/snapshots/ui/*.png` (all six: new greys, text and accent),
  `docs/ops.md` (2 new operations).

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `-p gradiance-ui`,
`-p gradiance-domain` (incl. doctests), `-p gradiance-interaction`, and the
`it` modules ui_snapshots (lavapipe, `GRADIANCE_REQUIRE_GPU=1`),
op_conformance, review, look, ui_panels, ui_groups, ui_registry, ui_input,
ui_conflicts: all green. rustc 1.100.0 (pinned).
