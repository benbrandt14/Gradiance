//! Settings window: Algodoo-style tabs, reflection-driven contents.
//!
//! Each tab is `reflect_grid(resource)` — new settings fields appear in
//! the UI automatically. Enums (not reflect-derivable into widgets) get
//! explicit rows; that is the sanctioned escape hatch.

use crate::fonts::glyph;
use crate::reflect_grid::{reflect_grid, reflect_grid_units};
use crate::widgets;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::StableId;
use gradiance_core::states::ToolState;
use gradiance_domain::Body;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::settings::{
    DebugSettings, GridSettings, GridSystem, KeyLightSettings, LightingSettings, RenderSettings,
    ScenerySettings, SimSettings, SnapConfig, ToolDefaults,
};
use gradiance_domain::shape::ShapeDef;
use gradiance_units::Dimension;

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
    history: Res<'w, gradiance_command::HistoryInfo>,
    selection: Res<'w, gradiance_interaction::selection::Selection>,
    tool: Res<'w, State<ToolState>>,
    snapped: Res<'w, gradiance_interaction::snap::SnappedCursor>,
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
                    reflect_grid_units(
                        ui,
                        egui::Id::new("sim"),
                        sim.bypass_change_detection(),
                        &[
                            ("gravity", Dimension::Acceleration.symbol()),
                            ("timestep_hz", Dimension::Frequency.symbol()),
                        ],
                    );
                    // Only flag the resource changed when egui actually
                    // edited something is overkill here; mark it touched.
                    sim.set_changed();
                }
                SettingsTab::GridSnap => {
                    grid_snap_tab(ui, &mut grid, &mut snap, &mut tool_defaults);
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

/// The Grid & Snap tab: the grid-system picker (an enum escape hatch) plus
/// the grid, snapping, and tool-default reflect grids with SI unit labels.
fn grid_snap_tab(
    ui: &mut egui::Ui,
    grid: &mut ResMut<GridSettings>,
    snap: &mut ResMut<SnapConfig>,
    tool_defaults: &mut ResMut<ToolDefaults>,
) {
    widgets::section_header(ui, "Grid");
    // Enum escape hatch: explicit variant picker.
    ui.horizontal(|ui| {
        ui.label("system");
        let current = grid.system;
        egui::ComboBox::from_id_salt("grid-system")
            .selected_text(format!("{current:?}"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut grid.system, GridSystem::Cartesian, "Cartesian");
                ui.selectable_value(&mut grid.system, GridSystem::Isometric, "Isometric");
                ui.selectable_value(
                    &mut grid.system,
                    GridSystem::Polar {
                        angular_divisions: 12,
                    },
                    "Polar",
                );
            });
    });
    reflect_grid_units(
        ui,
        egui::Id::new("grid"),
        grid.bypass_change_detection(),
        &[
            ("spacing", Dimension::Length.symbol()),
            ("origin", Dimension::Length.symbol()),
        ],
    );
    grid.set_changed();
    ui.separator();
    widgets::section_header(ui, "Snapping");
    // `max_screen_distance` is a screen-pixel capture radius, not a world
    // length — deliberately unlabelled.
    reflect_grid_units(
        ui,
        egui::Id::new("snap"),
        snap.bypass_change_detection(),
        &[("rotation_step_deg", "°")],
    );
    snap.set_changed();
    ui.separator();
    widgets::section_header(ui, "Tools");
    reflect_grid(
        ui,
        egui::Id::new("tool-defaults"),
        tool_defaults.bypass_change_detection(),
    );
    tool_defaults.set_changed();
}

/// The Lighting tab: per-light rows (sun gadget, color, strength,
/// shadows), scene scalars, and the backdrop settings. Hand-written —
/// the light list and colors are beyond the reflect grid.
fn lighting_tab(
    ui: &mut egui::Ui,
    lighting: &mut ResMut<LightingSettings>,
    scenery: &mut ResMut<ScenerySettings>,
) {
    let settings = lighting.bypass_change_detection();
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let count = settings.lights.len();
    for (index, light) in settings.lights.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Light {}", index + 1)).strong());
            if count > 1 && widgets::close_button(ui, "remove") {
                remove = Some(index);
            }
        });
        ui.horizontal(|ui| {
            changed |= sun_gadget(ui, index, light);
            ui.vertical(|ui| {
                changed |= color_row(ui, "color", &mut light.color);
                ui.horizontal(|ui| {
                    ui.label("strength");
                    changed |= ui
                        .add(egui::DragValue::new(&mut light.illuminance).speed(100.0))
                        .changed();
                });
                changed |= ui.checkbox(&mut light.shadows, "casts shadows").changed();
            });
        });
    }
    if let Some(index) = remove {
        settings.lights.remove(index);
        changed = true;
    }
    if settings.lights.len() < 4 && ui.button("+ add light (colored shadows)").clicked() {
        // A second light offset from the first, tinted, gives overlapping
        // colored shadows — cheap depth on a flat scene.
        settings.lights.push(KeyLightSettings {
            azimuth_deg: 40.0,
            color: gradiance_domain::appearance::Rgba::rgb(0.9, 0.85, 1.0),
            illuminance: 6_000.0,
            ..KeyLightSettings::default()
        });
        changed = true;
    }

    ui.separator();
    widgets::section_header(ui, "Scene");
    ui.horizontal(|ui| {
        ui.label("ambient");
        changed |= ui
            .add(egui::DragValue::new(&mut settings.ambient).speed(5.0))
            .changed();
    });
    changed |= ui
        .checkbox(&mut settings.ssao, "ambient occlusion (SSAO)")
        .changed();
    changed |= ui
        .checkbox(&mut settings.contact_shadows, "contact shadows")
        .changed();
    ui.horizontal(|ui| {
        ui.label("shadow sharpness");
        for (label, size) in [
            ("soft", 1024_u32),
            ("med", 2048),
            ("hard", 4096),
            ("max", 8192),
        ] {
            if ui
                .selectable_label(settings.shadow_map_size == size, label)
                .clicked()
            {
                settings.shadow_map_size = size;
                changed = true;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("shadow reach");
        changed |= ui
            .add(
                egui::DragValue::new(&mut settings.shadow_distance)
                    .speed(2.0)
                    .suffix(" m"),
            )
            .on_hover_text("distance shadows stay valid; smaller = crisper, too small clips")
            .changed();
    });
    if changed {
        lighting.set_changed();
    }

    ui.separator();
    widgets::section_header(ui, "Backdrop");
    color_row(
        ui,
        "back plane color",
        &mut scenery.bypass_change_detection().back_color,
    );
    reflect_grid_units(
        ui,
        egui::Id::new("scenery"),
        scenery.bypass_change_detection(),
        &[
            ("back_offset", Dimension::Length.symbol()),
            ("perspective_deg", "°"),
        ],
    );
    // reflect_grid edits aren't reported; conservatively mark touched.
    scenery.set_changed();
}

/// One labelled RGBA color-picker row over a domain [`Rgba`]; returns
/// whether it changed.
fn color_row(
    ui: &mut egui::Ui,
    label: &str,
    color: &mut gradiance_domain::appearance::Rgba,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut rgba = [color.r, color.g, color.b, color.a];
        if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
            [color.r, color.g, color.b, color.a] = rgba;
            changed = true;
        }
    });
    changed
}

/// A draggable sun-position gadget for one light: the handle's angle around
/// the circle is the azimuth; its distance from the rim toward the center is
/// the elevation (center = head-on, rim = grazing). Returns whether it moved.
fn sun_gadget(ui: &mut egui::Ui, index: usize, light: &mut KeyLightSettings) -> bool {
    const RADIUS: f32 = 40.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(RADIUS * 2.2, RADIUS * 2.2),
        egui::Sense::click_and_drag(),
    );
    let _ = index;
    let center = rect.center();
    let mut changed = false;

    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let v = pos - center;
        let r = (v.length() / RADIUS).clamp(0.0, 1.0);
        // Screen y is down; azimuth is measured in world (y-up) terms.
        light.azimuth_deg = f32::atan2(-v.y, v.x).to_degrees();
        light.elevation_deg = (1.0 - r) * 90.0;
        changed = true;
    }

    let painter = ui.painter_at(rect);
    painter.circle_stroke(center, RADIUS, egui::Stroke::new(1.0, egui::Color32::GRAY));
    painter.circle_stroke(
        center,
        RADIUS * 0.5,
        egui::Stroke::new(0.5, egui::Color32::from_gray(90)),
    );
    let azimuth = light.azimuth_deg.to_radians();
    let r = (1.0 - (light.elevation_deg / 90.0).clamp(0.0, 1.0)) * RADIUS;
    let handle = center + egui::vec2(azimuth.cos(), -azimuth.sin()) * r;
    painter.line_segment(
        [center, handle],
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
    );
    let tint = egui::Color32::from_rgb(
        (light.color.r * 255.0) as u8,
        (light.color.g * 255.0) as u8,
        (light.color.b * 255.0) as u8,
    );
    painter.circle_filled(handle, 6.0, tint);
    response.on_hover_text("drag the sun: angle = light direction, center = head-on");
    changed
}

/// One line per internals fact — the "what is the editor actually doing"
/// readout that grounds bug reports.
fn debug_readouts(ui: &mut egui::Ui, r: &DebugReadouts) {
    widgets::section_header(ui, "Internals");
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
    widgets::section_header(ui, "Selection");
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
    widgets::section_header(ui, "Joints (authored)");
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
        ui.label(format!(
            "{id:.8}: {kind} {} {:.8} to {target}",
            glyph::MIDDOT,
            def.body_a
        ));
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
