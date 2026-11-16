//! Settings window: Algodoo-style tabs, reflection-driven contents.
//!
//! Each tab is `reflect_grid(resource)` — new settings fields appear in
//! the UI automatically. Enums (not reflect-derivable into widgets) get
//! explicit rows; that is the sanctioned escape hatch.

use crate::fonts::glyph;
use crate::labels::short_id;
use crate::reflect_grid::{reflect_grid, reflect_grid_except, reflect_grid_units};
use crate::widgets;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use gradiance_core::ids::StableId;
use gradiance_core::states::ToolState;
use gradiance_domain::Body;
use gradiance_domain::joint::{JointDef, JointKind};
use gradiance_domain::settings::{
    CameraSettings, DebugSettings, GridSettings, GridSystem, KeyLightSettings, LightingSettings,
    RenderSettings, ScenerySettings, SimSettings, SnapConfig, ToolDefaults,
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
    /// Camera feel (zoom, pan and orbit sensitivity).
    Camera,
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

crate::impl_panel_toggle!(SettingsWindow, open);

/// Renders the tabbed settings window.
pub fn settings_window(
    mut contexts: EguiContexts,
    mut window: ResMut<SettingsWindow>,
    mut sim: ResMut<SimSettings>,
    mut grid: ResMut<GridSettings>,
    mut snap: ResMut<SnapConfig>,
    mut tool_defaults: ResMut<ToolDefaults>,
    mut camera: ResMut<CameraSettings>,
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
                ui.selectable_value(&mut window.tab, SettingsTab::Camera, "Camera");
                ui.selectable_value(&mut window.tab, SettingsTab::Rendering, "Rendering");
                ui.selectable_value(&mut window.tab, SettingsTab::Lighting, "Lighting");
                ui.selectable_value(&mut window.tab, SettingsTab::Debug, "Debug");
            });
            ui.separator();
            match window.tab {
                SettingsTab::Simulation => {
                    simulation_tab(ui, sim.bypass_change_detection());
                    // `commit_settings_edits` compares by value, so marking
                    // the resource touched every frame is harmless.
                    sim.set_changed();
                }
                SettingsTab::GridSnap => {
                    grid_snap_tab(ui, &mut grid, &mut snap, &mut tool_defaults);
                }
                SettingsTab::Camera => {
                    if camera_tab(ui, camera.bypass_change_detection()) {
                        camera.set_changed();
                    }
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
                    ui.separator();
                    debug_readouts(ui, &readouts);
                }
            }
        });
    window.open = open;
    Ok(())
}

/// `SimSettings` fields [`simulation_tab`] draws by hand; any field added
/// later still appears, through the reflect grid below them.
pub(crate) const SIM_HAND_DRAWN: &[&str] = &[
    "gravity",
    "speed",
    "substeps",
    "plane_friction",
    "timestep_hz",
];

/// The Simulation tab: the gravity-direction dial plus scrub-drag rows
/// (`labelled_drag` — the config seam; a settled edit becomes one undo step
/// through `commit_settings_edits`, so no gesture bookkeeping here).
fn simulation_tab(ui: &mut egui::Ui, sim: &mut SimSettings) {
    widgets::section_header(ui, "Gravity");
    let (mut angle, mut magnitude) = sim.gravity_polar();
    let mut gravity_changed = false;
    ui.horizontal(|ui| {
        gravity_changed |= gravity_dial(ui, &mut angle, magnitude);
        egui::Grid::new("sim-gravity")
            .num_columns(2)
            .show(ui, |ui| {
                gravity_changed |= widgets::labelled_drag(
                    ui,
                    &format!("direction ({})", glyph::DEGREE),
                    &mut angle,
                    -180.0..=180.0,
                    1.0,
                    "counter-clockwise from +x; straight down is -90",
                );
                gravity_changed |= widgets::labelled_drag(
                    ui,
                    &format!("strength ({})", Dimension::Acceleration.symbol()),
                    &mut magnitude,
                    0.0..=1000.0,
                    0.1,
                    "gravity magnitude; 0 = weightless",
                );
                ui.label(
                    egui::RichText::new(format!("x {:.2}  y {:.2}", sim.gravity.x, sim.gravity.y))
                        .weak(),
                );
                ui.end_row();
            });
    });
    if gravity_changed {
        sim.gravity = SimSettings::gravity_from_polar(angle, magnitude);
    }
    ui.separator();
    widgets::section_header(ui, "Solver");
    egui::Grid::new("sim-scalars")
        .num_columns(2)
        .show(ui, |ui| {
            widgets::labelled_drag(
                ui,
                "speed",
                &mut sim.speed,
                0.0..=10.0,
                0.01,
                "simulation speed multiplier; 1 = real time",
            );
            widgets::labelled_drag_u32(
                ui,
                "substeps",
                &mut sim.substeps,
                1..=64,
                "solver substeps per physics step: stiffer, more accurate, slower",
            );
            widgets::labelled_drag(
                ui,
                &format!("timestep ({})", Dimension::Frequency.symbol()),
                &mut sim.timestep_hz,
                10.0..=480.0,
                1.0,
                "physics steps per second",
            );
            widgets::labelled_drag(
                ui,
                "plane friction",
                &mut sim.plane_friction,
                0.0..=5.0,
                0.01,
                "Coulomb friction against the back plane (top-down mode); 0 = off",
            );
        });
    reflect_grid_except(ui, egui::Id::new("sim"), sim, &[], SIM_HAND_DRAWN);
}

/// A gravity-direction dial: the handle's angle is the gravity direction.
/// Click or drag on it to aim. Returns whether the angle moved.
fn gravity_dial(ui: &mut egui::Ui, angle_deg: &mut f32, magnitude: f32) -> bool {
    const RADIUS: f32 = 32.0;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(RADIUS * 2.2, RADIUS * 2.2),
        egui::Sense::click_and_drag(),
    );
    let center = rect.center();
    let mut changed = false;
    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let v = pos - center;
        if v.length() > 2.0 {
            // Screen y is down; the angle is in world (y-up) terms.
            *angle_deg = f32::atan2(-v.y, v.x).to_degrees();
            changed = true;
        }
    }
    let painter = ui.painter_at(rect);
    painter.circle_stroke(center, RADIUS, egui::Stroke::new(1.0, egui::Color32::GRAY));
    let a = angle_deg.to_radians();
    let dir = egui::vec2(a.cos(), -a.sin());
    // Weightless: a dot, not an arrow — there is no direction to show.
    let color = if magnitude > f32::EPSILON {
        ui.visuals().strong_text_color()
    } else {
        egui::Color32::from_gray(90)
    };
    painter.arrow(center, dir * RADIUS, egui::Stroke::new(2.0, color));
    response.on_hover_text("drag to aim gravity");
    changed
}

/// The Camera tab: camera feel (workstation config — saved with the scene,
/// never undone). Returns whether anything changed.
fn camera_tab(ui: &mut egui::Ui, camera: &mut CameraSettings) -> bool {
    let mut changed = false;
    let (lo, hi) = CameraSettings::ZOOM_STEP_RANGE;
    egui::Grid::new("camera").num_columns(2).show(ui, |ui| {
        changed |= widgets::labelled_drag(
            ui,
            "zoom step",
            &mut camera.zoom_step,
            lo..=hi,
            0.002,
            "zoom multiplier per scroll notch: closer to 1 is gentler",
        );
        changed |= widgets::labelled_drag(
            ui,
            "pan speed (px/s)",
            &mut camera.pan_speed,
            0.0..=5000.0,
            5.0,
            "arrow-key pan speed in screen pixels per second",
        );
        changed |= widgets::labelled_drag(
            ui,
            &format!("orbit speed ({}/px)", glyph::DEGREE),
            &mut camera.orbit_speed_deg,
            0.0..=5.0,
            0.005,
            "middle-drag orbit sensitivity",
        );
    });
    if ui.button("Reset to defaults").clicked() {
        *camera = CameraSettings::default();
        changed = true;
    }
    widgets::hint(
        ui,
        &format!(
            "right-drag or arrows pan {} wheel zooms {} middle-drag orbits {} Home glides back to 2D",
            glyph::MIDDOT,
            glyph::MIDDOT,
            glyph::MIDDOT
        ),
    );
    changed
}

/// `GridSettings` fields [`grid_snap_tab`] draws by hand, so the reflect
/// grid below them leaves them out.
pub(crate) const GRID_HAND_DRAWN: &[&str] = &["system"];

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
    reflect_grid_except(
        ui,
        egui::Id::new("grid"),
        grid.bypass_change_detection(),
        &[
            ("spacing", Dimension::Length.symbol()),
            ("origin", Dimension::Length.symbol()),
        ],
        GRID_HAND_DRAWN,
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
        .map(short_id)
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
                format!("Spring rest={rest_length:.0} k={stiffness:.0} c={damping:.1}{clamp}")
            }
        };
        let target = def.body_b.map_or("world pin".to_owned(), |b| short_id(&b));
        ui.label(format!(
            "{}: {kind} {} {} to {target}",
            short_id(id),
            glyph::MIDDOT,
            short_id(&def.body_a)
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
        ShapeDef::Capsule {
            half_length,
            radius,
        } => format!("Capsule l={:.1} r={radius:.1}", 2.0 * half_length),
        ShapeDef::HalfPlane => "HalfPlane".to_owned(),
        ShapeDef::Csg { op, lhs, rhs } => {
            format!("({} {op:?} {})", shape_summary(lhs), shape_summary(rhs))
        }
        ShapeDef::Placed { shape, .. } => format!("Placed({})", shape_summary(shape)),
    }
}
