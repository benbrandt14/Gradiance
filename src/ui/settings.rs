//! Settings window: Algodoo-style tabs, reflection-driven contents.
//!
//! Each tab is `reflect_grid(resource)` — new settings fields appear in
//! the UI automatically. Enums (not reflect-derivable into widgets) get
//! explicit rows; that is the sanctioned escape hatch.

use crate::core::ids::StableId;
use crate::core::states::ToolState;
use crate::domain::Body;
use crate::domain::joint::{JointDef, JointKind};
use crate::domain::settings::{
    DebugSettings, GridSettings, GridSystem, RenderSettings, SimSettings, SnapConfig,
};
use crate::domain::shape::ShapeDef;
use crate::ui::reflect_grid::reflect_grid;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

/// Which settings tab is open.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    /// Simulation tuning.
    #[default]
    Simulation,
    /// Grid & snapping.
    GridSnap,
    /// Rendering style.
    Rendering,
    /// Debug overlays and internals readouts.
    Debug,
}

/// Read-only internals shown by the Debug tab.
#[derive(bevy::ecs::system::SystemParam)]
pub struct DebugReadouts<'w, 's> {
    history: Res<'w, crate::command::HistoryInfo>,
    selection: Res<'w, crate::interaction::selection::Selection>,
    tool: Res<'w, State<ToolState>>,
    snapped: Res<'w, crate::interaction::snap::SnappedCursor>,
    diagnostics: Option<Res<'w, DiagnosticsStore>>,
    bodies: Query<'w, 's, (&'static StableId, &'static ShapeDef), With<Body>>,
    joints: Query<'w, 's, (&'static StableId, &'static JointDef)>,
    ids: Query<'w, 's, &'static StableId>,
}

/// Settings window state.
#[derive(Resource, Default, Debug)]
pub struct SettingsWindow {
    /// Window visibility.
    pub open: bool,
    /// Active tab.
    pub tab: SettingsTab,
}

/// Renders the tabbed settings window.
pub fn settings_window(
    mut contexts: EguiContexts,
    mut window: ResMut<SettingsWindow>,
    mut sim: ResMut<SimSettings>,
    mut grid: ResMut<GridSettings>,
    mut snap: ResMut<SnapConfig>,
    mut render: ResMut<RenderSettings>,
    mut debug: ResMut<DebugSettings>,
    readouts: DebugReadouts,
) -> Result {
    let ctx = contexts.ctx_mut()?;

    if !window.open {
        return Ok(());
    }
    let mut open = window.open;
    egui::Window::new("Settings")
        .open(&mut open)
        .default_width(320.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut window.tab, SettingsTab::Simulation, "Simulation");
                ui.selectable_value(&mut window.tab, SettingsTab::GridSnap, "Grid & Snap");
                ui.selectable_value(&mut window.tab, SettingsTab::Rendering, "Rendering");
                ui.selectable_value(&mut window.tab, SettingsTab::Debug, "Debug");
            });
            ui.separator();
            match window.tab {
                SettingsTab::Simulation => {
                    reflect_grid(ui, egui::Id::new("sim"), sim.bypass_change_detection());
                    // Only flag the resource changed when egui actually
                    // edited something is overkill here; mark it touched.
                    sim.set_changed();
                }
                SettingsTab::GridSnap => {
                    ui.label(egui::RichText::new("Grid").strong());
                    // Enum escape hatch: explicit variant picker.
                    ui.horizontal(|ui| {
                        ui.label("system");
                        let current = grid.system;
                        egui::ComboBox::from_id_salt("grid-system")
                            .selected_text(format!("{current:?}"))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Cartesian,
                                    "Cartesian",
                                );
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Isometric,
                                    "Isometric",
                                );
                                ui.selectable_value(
                                    &mut grid.system,
                                    GridSystem::Polar {
                                        angular_divisions: 12,
                                    },
                                    "Polar",
                                );
                            });
                    });
                    reflect_grid(ui, egui::Id::new("grid"), grid.bypass_change_detection());
                    grid.set_changed();
                    ui.separator();
                    ui.label(egui::RichText::new("Snapping").strong());
                    reflect_grid(ui, egui::Id::new("snap"), snap.bypass_change_detection());
                    snap.set_changed();
                }
                SettingsTab::Rendering => {
                    reflect_grid(
                        ui,
                        egui::Id::new("render"),
                        render.bypass_change_detection(),
                    );
                    render.set_changed();
                }
                SettingsTab::Debug => {
                    reflect_grid(ui, egui::Id::new("debug"), debug.bypass_change_detection());
                    debug.set_changed();
                    ui.label(
                        egui::RichText::new(
                            "middle-drag orbits the 3D view · Home glides back to 2D",
                        )
                        .weak(),
                    );
                    ui.separator();
                    debug_readouts(ui, &readouts);
                }
            }
        });
    window.open = open;
    Ok(())
}

/// One line per internals fact — the "what is the editor actually doing"
/// readout that grounds bug reports.
fn debug_readouts(ui: &mut egui::Ui, r: &DebugReadouts) {
    ui.label(egui::RichText::new("Internals").strong());
    if let Some(fps) = r
        .diagnostics
        .as_ref()
        .and_then(|d| d.get(&FrameTimeDiagnosticsPlugin::FPS))
        .and_then(bevy::diagnostic::Diagnostic::smoothed)
    {
        ui.label(format!("fps: {fps:.0}"));
    }
    ui.label(format!(
        "bodies: {} · joints: {} · undo: {} · redo: {}",
        r.bodies.iter().count(),
        r.joints.iter().count(),
        r.history.undo_depth,
        r.history.redo_depth,
    ));
    ui.label(format!("tool: {:?}", r.tool.get()));
    ui.label(format!(
        "cursor: {:?} · snap: {:?}",
        r.snapped.effective().map(|p| (p.x.round(), p.y.round())),
        r.snapped.kind,
    ));

    ui.separator();
    ui.label(egui::RichText::new("Selection").strong());
    let ids: Vec<String> = r
        .selection
        .iter()
        .filter_map(|e| r.ids.get(e).ok())
        .map(|id| format!("{id:.8}"))
        .collect();
    ui.label(format!("{} selected: {}", ids.len(), ids.join(", ")));
    if let Some(primary) = r.selection.primary()
        && let Ok((_, shape)) = r.bodies.get(primary)
    {
        ui.label(format!("primary shape: {}", shape_summary(shape)));
    }

    ui.separator();
    ui.label(egui::RichText::new("Joints (authored)").strong());
    for (id, def) in &r.joints {
        let kind = match &def.kind {
            JointKind::Hinge { limits, motor } => format!(
                "Hinge{}{}",
                if limits.is_some() { " +limits" } else { "" },
                if motor.is_some() { " +motor" } else { "" },
            ),
            JointKind::Weld => "Weld".to_owned(),
            JointKind::Slider { limits, motor, .. } => format!(
                "Slider{}{}",
                if limits.is_some() { " +limits" } else { "" },
                if motor.is_some() { " +motor" } else { "" },
            ),
            JointKind::Spring {
                rest_length,
                stiffness,
                damping,
                range,
            } => {
                let clamp = range.map_or(String::new(), |[lo, hi]| format!(" [{lo:.0},{hi:.0}]"));
                format!("Strut rest={rest_length:.0} k={stiffness:.0} c={damping:.1}{clamp}")
            }
        };
        let target = def
            .body_b
            .map_or("world pin".to_owned(), |b| format!("{b:.8}"));
        ui.label(format!("{id:.8}: {kind} · {:.8} ↔ {target}", def.body_a));
    }
}

/// Compact one-line description of a shape tree.
fn shape_summary(shape: &ShapeDef) -> String {
    match shape {
        ShapeDef::Box { width, height } => format!("Box {width:.1}×{height:.1}"),
        ShapeDef::Circle { radius } => format!("Circle r={radius:.1}"),
        ShapeDef::Polygon { outline, holes } => {
            format!("Polygon {}v {}h", outline.len(), holes.len())
        }
        ShapeDef::HalfPlane => "HalfPlane".to_owned(),
        ShapeDef::Csg { op, lhs, rhs } => {
            format!("({} {op:?} {})", shape_summary(lhs), shape_summary(rhs))
        }
        ShapeDef::Placed { shape, .. } => format!("Placed({})", shape_summary(shape)),
    }
}
