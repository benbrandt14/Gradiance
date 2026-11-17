# Image snapshot tests of the editor UI on lavapipe
Phase U · `docs/unified-surface-plan.md` § 10 row 6 ("**U-5** snapshot
tests") and § 8 tier T1, snapshot half · status: in work

Written by the worker (owner ruling "scoped work", `docs/state.md`: a
session may write the brief when the work is in the plan).

## Goal

Later visual briefs (4b-2 groups panel, 4b-3 art direction, U-7 theme,
U-8..U-11) change how the editor looks. Each of them must be checkable in an
agent container and in CI, without a person looking at a screenshot. This
brief adds **image snapshot tests**: a handful of the editor's egui panels
render on a real wgpu adapter (Mesa's **lavapipe**, a CPU Vulkan driver) at a
fixed size with the editor's own fonts, and each frame is compared with a PNG
committed under `tests/snapshots/ui/`. A change in layout, theme or glyphs
then fails a test, and the diff image says where.

## Read first

- `docs/unified-surface-plan.md` § 6 (porting rules), § 8 (tier T1:
  `egui_kittest` `snapshot` + `wgpu` features, Craft `Screens` harness, a GPU
  test lock, CI fails if no adapter).
- `docs/briefs/U-3-widget-registry-inspect-screenshot.md` and
  `docs/briefs/U-4-input-injection.md` Outcomes (the T2 half; the porting
  decisions; upstream reachability).
- `tests/it/ui_panels.rs` (the existing `egui_kittest` accessibility tests:
  the leaf renderers this brief snapshots).
- `crates/gradiance-ui/src/fonts.rs` (`fonts::install`, `glyph::ALL`).
- `docs/ui-design.md` (what the panels are meant to look like).

## Facts (worker, 2026-11-17, measured on `38541ff`)

- `egui_kittest` is pinned `=0.36.2` and used without features by
  `gradiance-ui` (dev) and the root package (dev, `tests/it`). Its `wgpu`
  feature depends on `egui-wgpu 0.36.2` and on **`wgpu 30`, the version Bevy
  0.20.1 already uses**: no second wgpu. Its `snapshot` feature adds `dify`,
  `image`, `open`, `tempfile`.
- Enabling both on the root dev-dependency adds 13 lockfile entries
  (`egui-wgpu`, `dify`, `pollster`, `open`, …; `eframe` and `egui-winit` are
  lockfile-only and are not built) and adds the wgpu features `gles` and
  `std`. Feature unification means test builds compile wgpu and the Bevy
  render crates once more than `cargo build --bin gradiance` does (disk, and
  one CI cache rotation).
- `egui_kittest::wgpu::default_wgpu_setup` prefers a CPU adapter, then a
  discrete GPU; `create_render_state` **panics** when no adapter is found.
  Snapshots go to `tests/snapshots/<name>.png` relative to the working
  directory unless `SnapshotOptions::output_path` says otherwise; default
  tolerance: per-pixel threshold 0.6 (YIQ distance), 0 failing pixels;
  `UPDATE_SNAPSHOTS=1` rewrites the files.
- The container has `mesa-vulkan-drivers 25.2.8-0ubuntu0.24.04.5` and
  `libvulkan1` (Ubuntu 24.04, as `ubuntu-latest` in CI). CI's `ci` job
  installs neither today.
- The editor installs no custom visuals: it runs egui's default theme with
  `fonts::install` (Hack appended to the proportional chain).
- **Upstreams:** `tangency-labs/craft-libs` (the `Screens` harness) still
  asks for credentials on `git ls-remote` (private or absent). SolidCraft
  (`e424409`) has no snapshot harness in code; its `SOLIDCRAFT_REQUIRE_GPU=1`
  rule is documented in `docs/render-path.md` and `docs/blur-behind.md`: with
  the variable set a missing adapter fails the GPU tests, without it they
  skip.
- Not checked: whether CI's lavapipe renders bit-identically to the
  container's (the first CI run measures it).

## Approach

1. Root dev-dependency `egui_kittest` gains `snapshot` and `wgpu`
   (`Cargo.lock` changes; reviewed).
2. `tests/it/ui_snapshots.rs`: one helper builds a `Harness` at a fixed size
   with the editor's fonts and dark theme, a wgpu renderer, and the
   project's tolerance. The **GPU gate** probes the adapter once:
   - adapter found: render and compare;
   - none, `GRADIANCE_REQUIRE_GPU=1`: the test fails with the reason;
   - none, variable unset: the test prints why and returns (a laptop
     without Vulkan can still run the suite).
   A process-wide **GPU lock** serialises the snapshot tests (one lavapipe
   device at a time; the umbrella binary runs tests in parallel).
3. A handful of snapshots, each a leaf renderer the editor really draws:
   the tool palette, the settings reflect grid, the curve editor, the plot
   pane, and a glyph sheet (`glyph::ALL` in the proportional font, so a
   tofu box shows as a pixel diff).
4. CI: install `mesa-vulkan-drivers` and set `GRADIANCE_REQUIRE_GPU=1` in
   the jobs that run tests.
5. Docs: the update command and tolerance in `tools/screenshot/README.md`;
   the porting decision in `docs/upstream-watch.md`.

Invariants: egui stays in `gradiance-ui` and the root dev-dependencies
(`tests/boundaries.rs` already allows the root); no new crate edge; no
change to the app.

## Out of scope

- Theme changes (U-7), `panel.*` operations (U-3b), CLI/MCP (U-6), new
  widgets.
- Snapshots of whole Bevy-system panels (they need an app and an egui pass
  inside Bevy); a full-app snapshot through `ui.screenshot` (see Outcome).
- A snapshot per panel: a handful, small.

## Done when

- `tests/it/ui_snapshots.rs` renders and compares the snapshots on lavapipe
  locally; the committed PNGs are small (fixed size, a few KB each).
- A test of the gate: with the variable set, a missing adapter is an error
  (pure function, no GPU needed).
- CI installs lavapipe, sets `GRADIANCE_REQUIRE_GPU=1`, and is green.
- `tools/screenshot/README.md`: how to update the snapshots and the
  tolerance; `docs/upstream-watch.md`: the porting decision.
- fmt, clippy, doc, `tests/boundaries.rs` local. Outcome filled.

## Outcome

(filled at handoff)
