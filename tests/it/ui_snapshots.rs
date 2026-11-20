//! Image snapshot tests of the editor's egui panels (brief U-5, plan § 8 T1).
//!
//! Each test draws one leaf renderer the editor really uses, at a fixed size
//! with the editor's fonts and theme, on a real wgpu adapter (in a container
//! and in CI: Mesa's **lavapipe**, a CPU Vulkan driver), and compares the
//! frame with a PNG committed under `tests/snapshots/ui/`. A change in layout,
//! theme or glyphs fails the test; `egui_kittest` writes `<name>.new.png` and
//! `<name>.diff.png` next to the committed file to show where.
//!
//! Update after an intended visual change (then review the PNG diff):
//!
//! ```sh
//! UPDATE_SNAPSHOTS=1 cargo test --test it ui_snapshots
//! ```
//!
//! **GPU gate** (`SolidCraft`'s `SOLIDCRAFT_REQUIRE_GPU` rule, ported as a
//! rule): with `GRADIANCE_REQUIRE_GPU=1` (CI sets it) a missing adapter fails
//! every snapshot test; without it they print why and pass, so a machine
//! without Vulkan can still run the suite. **GPU lock:** the umbrella binary
//! runs tests in parallel; the snapshot tests take one process-wide lock so a
//! single lavapipe device renders at a time.

use bevy_egui::egui;
use egui_kittest::wgpu::WgpuTestRenderer;
use egui_kittest::{Harness, SnapshotOptions};
use gradiance::core::states::ToolState;
use gradiance::domain::signal::Curve;
use gradiance::ui::curve::curve_editor;
use gradiance::ui::fonts::{self, glyph};
use gradiance::ui::plot::{PlotConfig, Series, plot_section};
use gradiance::ui::reflect_grid::reflect_grid;
use gradiance::ui::theme;
use gradiance::ui::toolbar::tools_palette_ui;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

/// The environment variable that turns a missing adapter into a failure.
const REQUIRE_GPU: &str = "GRADIANCE_REQUIRE_GPU";

/// Per-pixel tolerance: the weighted squared YIQ distance a pixel may move
/// before it counts as different (`egui_kittest`'s default; a one-step
/// change of an egui theme colour is well above it).
const THRESHOLD: f32 = 0.6;

/// Pixels that may pass [`THRESHOLD`] before a snapshot fails: room for
/// anti-aliasing differences between Mesa releases, far below one changed
/// glyph (a 13 pt letter is about 60 pixels).
const MAX_FAILED_PIXELS: usize = 20;

/// What the gate decides for one test.
#[derive(Debug, PartialEq, Eq)]
enum Gate {
    /// An adapter exists: render and compare.
    Render,
    /// No adapter and none required: skip, with the reason.
    Skip(String),
}

/// The GPU gate as a pure function of the probe and the environment.
///
/// # Errors
/// The probe's reason, when an adapter is required and there is none.
fn gate(probe: &Result<(), String>, require: Option<&str>) -> Result<Gate, String> {
    match (probe, require) {
        (Ok(()), _) => Ok(Gate::Render),
        (Err(why), Some("1")) => Err(format!(
            "{REQUIRE_GPU}=1 and no wgpu adapter: {why} \
             (install mesa-vulkan-drivers for lavapipe)"
        )),
        (Err(why), _) => Ok(Gate::Skip(why.clone())),
    }
}

/// Probes for an adapter once per process. `egui_kittest`'s renderer panics
/// when it finds none, so the probe catches that panic.
fn probe() -> &'static Result<(), String> {
    static PROBE: OnceLock<Result<(), String>> = OnceLock::new();
    PROBE.get_or_init(|| {
        std::panic::catch_unwind(WgpuTestRenderer::new)
            .map(drop)
            .map_err(|panic| {
                panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_else(|| "the wgpu renderer could not be created".to_owned())
            })
    })
}

/// Holds the GPU lock for one snapshot test, or `None` when the gate skips.
fn gpu() -> Option<MutexGuard<'static, ()>> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    match gate(probe(), std::env::var(REQUIRE_GPU).ok().as_deref()) {
        Ok(Gate::Render) => Some(guard),
        Ok(Gate::Skip(why)) => {
            eprintln!(
                "ui_snapshots: skipped, no wgpu adapter ({why}); set {REQUIRE_GPU}=1 to fail"
            );
            None
        }
        Err(why) => panic!("{why}"),
    }
}

/// Renders `ui` in a central panel of `size` logical points (1 pixel per
/// point) with the editor's fonts and default chrome (`ui::theme`), and compares it with
/// `tests/snapshots/ui/<name>.png`.
fn snapshot(name: &str, size: egui::Vec2, mut ui: impl FnMut(&mut egui::Ui)) {
    let Some(_gpu) = gpu() else { return };
    let options = SnapshotOptions::new()
        .threshold(THRESHOLD)
        .max_failed_pixels(MAX_FAILED_PIXELS)
        .output_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/ui"));
    let mut harness = Harness::builder()
        .with_size(size)
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Dark)
        .wgpu()
        .build_ui(move |root| {
            egui::CentralPanel::default().show(root, &mut ui);
        });
    fonts::install(&harness.ctx);
    theme::install(&harness.ctx, &theme::ChromeTheme::default());
    harness.run();
    harness.snapshot_options(name, &options);
}

/// The gate: an adapter renders; a missing one skips unless required, and
/// then fails with the probe's reason. Pure, so it runs without a GPU.
#[test]
fn gpu_gate_fails_loudly_only_when_required() {
    let none = Err("No adapter found".to_owned());
    assert_eq!(gate(&Ok(()), Some("1")), Ok(Gate::Render));
    assert_eq!(gate(&Ok(()), None), Ok(Gate::Render));
    assert_eq!(gate(&none, None), Ok(Gate::Skip("No adapter found".into())));
    assert_eq!(
        gate(&none, Some("0")),
        Ok(Gate::Skip("No adapter found".into()))
    );
    let err = gate(&none, Some("1")).expect_err("required and missing");
    assert!(err.contains("No adapter found"), "{err}");
}

/// The tool palette, Box selected (the text fallback, as headless and CI
/// have no icon textures).
#[test]
fn snapshot_tool_palette() {
    snapshot("tool_palette", egui::vec2(120.0, 418.0), |ui| {
        let _ = tools_palette_ui(ui, ToolState::Box, None, &[]);
    });
}

/// The Simulation settings grid (the reflect grid every settings tab uses).
#[test]
fn snapshot_settings_grid() {
    let mut settings = gradiance::domain::settings::SimSettings::default();
    snapshot("settings_grid", egui::vec2(260.0, 230.0), move |ui| {
        reflect_grid(ui, egui::Id::new("sim"), &mut settings);
    });
}

/// The curve editor on the default curve.
#[test]
fn snapshot_curve_editor() {
    let mut curve = Curve::default();
    snapshot("curve_editor", egui::vec2(300.0, 220.0), move |ui| {
        let _ = curve_editor(ui, "snapshot", &mut curve);
    });
}

/// The plot pane with one sine series (fixed data: no clock involved).
#[test]
fn snapshot_plot_pane() {
    let wave: VecDeque<f32> = (0..200).map(|i| (i as f32 * 0.1).sin()).collect();
    let times: VecDeque<f32> = (0..200).map(|i| i as f32 * 0.016).collect();
    let mut config = PlotConfig::default();
    snapshot("plot_pane", egui::vec2(360.0, 240.0), move |ui| {
        let series = [Series {
            name: "wave",
            unit: gradiance::units::DisplayUnit::canonical(gradiance::units::Dimension::Length),
            values: &wave,
            times: &times,
        }];
        plot_section(ui, &series, &mut config);
    });
}

/// Every glyph the editor may use, in the proportional face labels use, so
/// a glyph that falls back to a tofu box is a pixel diff.
#[test]
fn snapshot_glyph_sheet() {
    snapshot("glyph_sheet", egui::vec2(420.0, 64.0), |ui| {
        ui.label(glyph::ALL.concat());
        ui.monospace(glyph::ALL.concat());
    });
}

/// The Groups panel (brief 4b-2) on the `ui_panels` fixture, its named inner
/// group active and the red chip on.
#[test]
fn snapshot_groups_panel() {
    use gradiance::ui::groups::{GroupsOutput, GroupsPanel, groups_section};
    let model = crate::ui_panels::groups_fixture();
    let mut panel = GroupsPanel::default();
    panel.set_active(Some(2));
    panel.set_clauses(vec!["palette:0".into()]);
    snapshot("groups_panel", egui::vec2(260.0, 440.0), move |ui| {
        let mut out = GroupsOutput::default();
        groups_section(ui, &model, &mut panel, &mut out);
    });
}
