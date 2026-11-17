# Image snapshot tests of the editor UI on lavapipe
Phase U · `docs/unified-surface-plan.md` § 10 row 6 ("**U-5** snapshot
tests") and § 8 tier T1, snapshot half · status: in review

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

**Landed (worker, 2026-11-17), PR in review.** `tests/it/ui_snapshots.rs`:
five image snapshots plus one pure test of the GPU gate. Each snapshot draws
a leaf renderer in a central panel at a fixed size, 1 pixel per point, dark
theme, `fonts::install`, on `egui_kittest`'s wgpu renderer (it prefers a CPU
adapter: lavapipe). Committed PNGs in `tests/snapshots/ui/` (73 KB in all):

| Snapshot | Size (px) | PNG |
|---|---|---|
| `tool_palette` (Box selected, text fallback) | 120x330 | 16 KB |
| `settings_grid` (`SimSettings` reflect grid) | 260x140 | 9 KB |
| `curve_editor` (default curve) | 300x220 | 14 KB |
| `plot_pane` (one sine series, fixed data) | 360x240 | 25 KB |
| `glyph_sheet` (`glyph::ALL`, proportional and monospace) | 420x64 | 8 KB |

**Porting decision: native, rule ported.** craft-libs (`Screens`) is still
unreachable (credentials asked). SolidCraft `e424409` has no snapshot
harness in code, only the `SOLIDCRAFT_REQUIRE_GPU` rule in its docs; that
rule is ported as a rule (`GRADIANCE_REQUIRE_GPU`), cited in the test's
module docs and `docs/upstream-watch.md`. Nothing added to
`vendor/UPSTREAM.toml` (it lists ported files only).

**Decisions (non-interactive, owner ruling 2026-11-17):**

1. *Where:* a module of the umbrella `tests/it` binary (one Bevy-sized
   link), not `gradiance-ui` unit tests: the root dev-dependency is the one
   that gains the features, and `tests/boundaries.rs` already exempts the
   root manifest.
2. *Gate:* the adapter is probed once per process by catching
   `egui_kittest`'s panic (`create_render_state` panics with no adapter;
   it exposes no fallible constructor). `GRADIANCE_REQUIRE_GPU=1`: a missing
   adapter fails every snapshot test with the probe's reason; unset: the test
   prints why and passes. Checked both ways locally by hiding the drivers
   (`VK_ICD_FILENAMES=/nonexistent`, `__EGL_VENDOR_LIBRARY_FILENAMES=/nonexistent`).
3. *GPU lock:* a process-wide `Mutex` held for the whole test (harness,
   run, render, compare), poison ignored.
4. *Tolerance:* per-pixel YIQ threshold 0.6 (upstream default), at most 20
   failing pixels per snapshot. Measured: moving the palette highlight from
   Box to Circle fails 1585 pixels at every threshold 0.0-10.0; four
   repeated runs on lavapipe were identical.
5. *Output path* from `CARGO_MANIFEST_DIR`, not the working directory.
   By-products (`*.new.png`, `*.diff.png`, `*.old.png`) are git-ignored; CI
   uploads them as the `ui-snapshot-diffs` artifact when Test fails.
6. *CI:* `mesa-vulkan-drivers libvulkan1` installed in the `ci` and
   `coverage` jobs; `GRADIANCE_REQUIRE_GPU=1` at workflow level (both run the
   tests).
7. *No full-app snapshot.* A Bevy-render snapshot through `ui.screenshot`
   needs the windowed app under Xvfb inside a test (start-up, render settle,
   readback; tens of seconds on lavapipe) and a different tolerance; not
   cheap enough for one test here. `tools/screenshot/session.sh` covers it
   ad hoc; a later brief can pin a scene.
8. *Dependency:* root dev-dependency `egui_kittest` gains `snapshot` and
   `wgpu` (pin unchanged). `Cargo.lock`: 13 new entries (`egui-wgpu`,
   `dify`, `pollster`, `open`, `type-map`, `colored`, `getopts`, `is-docker`,
   `is-wsl`, `objc2-ui-kit`, `wgpu-core-deps-emscripten`; `eframe` and
   `egui-winit` are lockfile-only, not built). wgpu stays 30.0.0; its
   features gain `gles` and `std`, so test builds rebuild the Bevy render
   crates once (7 min 52 s cold here, the `it` binary included) and keep a
   second copy beside `cargo build --bin gradiance`'s (its size was not
   measured apart from the rebuilt workspace crates).

**Checked locally** (rustc 1.100.0, mesa 25.2.8 lavapipe):
`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries` (8);
`GRADIANCE_REQUIRE_GPU=1 cargo test --test it -- ui_` (24, the snapshots
included).

**Not checked:** the full `cargo test --workspace` (CI); whether CI's
lavapipe matches the container's pixels (first CI run, see the PR);
HiDPI (snapshots are 1 pixel per point); the light theme; whole
Bevy-system panels (they need an app with an egui pass).

**Deviations from the brief:** none.
