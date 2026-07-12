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
    DebugSettings, GridSettings, GridSystem, LightingSettings, RenderSettings, ScenerySettings,
    SimSettings, SnapConfig, ToolDefaults,
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
    /// Key light, ambient, and backdrop planes.
    Lighting,
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
    mut tool_defaults: ResMut<ToolDefaults>,
    mut render: ResMut<RenderSettings>,
    mut lighting: ResMut<LightingSettings>,
    mut scenery: ResMut<ScenerySettings>,
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
                ui.selectable_value(&mut window.tab, SettingsTab::Lighting, "Lighting");
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
                    ui.separator();
                    ui.label(egui::RichText::new("Tools").strong());
                    reflect_grid(
                        ui,
                        egui::Id::new("tool-defaults"),
                        tool_defaults.bypass_change_detection(),
                    );
                    tool_defaults.set_changed();
                }
                SettingsTab::Rendering => {
                    reflect_grid(
                        ui,
                        egui::Id::new("render"),
                        render.bypass_change_detection(),
                    );
                    render.set_changed();
                }
                SettingsTab::Lighting => {
                    lighting_tab(ui, &mut lighting, &mut scenery);
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

/// The Lighting tab: a draggable sun gadget for the key light's angle,
/// color pickers for light and back plane, and the reflect grid for the
/// scalar fields.
fn lighting_tab(
    ui: &mut egui::Ui,
    lighting: &mut ResMut<LightingSettings>,
    scenery: &mut ResMut<ScenerySettings>,
) {
    ui.label(egui::RichText::new("Key light").strong());
    ui.horizontal(|ui| {
        sun_gadget(ui, lighting);
        ui.vertical(|ui| {
            color_row(ui, "color", &mut lighting.bypass_change_detection().color);
            reflect_grid(
                ui,
                egui::Id::new("lighting"),
                lighting.bypass_change_detection(),
            );
        });
    });
    lighting.set_changed();
    ui.separator();
    ui.label(egui::RichText::new("Backdrop").strong());
    color_row(
        ui,
        "back plane color",
        &mut scenery.bypass_change_detection().back_color,
    );
    reflect_grid(
        ui,
        egui::Id::new("scenery"),
        scenery.bypass_change_detection(),
    );
    scenery.set_changed();
}

/// One labelled RGBA color-picker row over a domain [`Rgba`]
/// (`reflect_grid` renders color structs as four bare floats — this is the
/// sanctioned escape hatch, like the grid-system picker).
fn color_row(ui: &mut egui::Ui, label: &str, color: &mut crate::domain::appearance::Rgba) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = [color.r, color.g, color.b, color.a];
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            [color.r, color.g, color.b, color.a] = rgba;
        }
    });
}

/// A draggable sun-position gadget: the handle's angle around the circle is
/// the light's azimuth; its distance from the rim toward the center is the
/// elevation (center = head-on, rim = grazing).
fn sun_gadget(ui: &mut egui::Ui, lighting: &mut ResMut<LightingSettings>) {
    const RADIUS: f32 = 44.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(RADIUS * 2.2, RADIUS * 2.2),
        egui::Sense::click_and_drag(),
    );
    let center = rect.center();

    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let v = pos - center;
        let r = (v.length() / RADIUS).clamp(0.0, 1.0);
        // Screen y is down; azimuth is measured in world (y-up) terms.
        lighting.azimuth_deg = f32::atan2(-v.y, v.x).to_degrees();
        lighting.elevation_deg = (1.0 - r) * 90.0;
    }
    let settings = lighting.bypass_change_detection();

    let painter = ui.painter_at(rect);
    painter.circle_stroke(center, RADIUS, egui::Stroke::new(1.0, egui::Color32::GRAY));
    painter.circle_stroke(
        center,
        RADIUS * 0.5,
        egui::Stroke::new(0.5, egui::Color32::from_gray(90)),
    );
    let azimuth = settings.azimuth_deg.to_radians();
    let r = (1.0 - (settings.elevation_deg / 90.0).clamp(0.0, 1.0)) * RADIUS;
    let handle = center + egui::vec2(azimuth.cos(), -azimuth.sin()) * r;
    painter.line_segment(
        [center, handle],
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
    );
    painter.circle_filled(handle, 6.0, egui::Color32::GOLD);
    response.on_hover_text("drag the sun: angle = light direction, center = head-on");
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
            JointKind::Slider { limits, motor, .. } => format!(
                "Prismatic{}{}",
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
