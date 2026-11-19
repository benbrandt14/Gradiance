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
use gradiance_units::{AngleUnit, Dimension, DisplayUnit, LengthUnit};

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
    /// The chrome preset (workstation config, brief U-7).
    Interface,
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
    mut chrome: ResMut<gradiance_domain::settings::ChromeSettings>,
    mut units: ResMut<gradiance_domain::settings::UnitSettings>,
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
                ui.selectable_value(&mut window.tab, SettingsTab::Interface, "Interface");
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
                    look_picker(ui, &mut render, &mut lighting, &mut scenery);
                    ui.separator();
                    ramp_section(ui, render.bypass_change_detection());
                    ui.separator();
                    // `look` is the picker above, `ramp` the ramp editor.
                    reflect_grid_except(
                        ui,
                        egui::Id::new("render"),
                        render.bypass_change_detection(),
                        &[],
                        &["look", "ramp"],
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
                SettingsTab::Interface => {
                    interface_tab(ui, &mut chrome);
                    ui.separator();
                    units_section(ui, &mut units);
                }
            }
        });
    window.open = open;
    Ok(())
}

/// The scene-look picker (brief 4b-3): choosing a look applies its whole
/// preset (render, lighting, scenery) — the same writes `look.apply` makes,
/// so a settled pick is one undo step through `commit_settings_edits`.
fn look_picker(
    ui: &mut egui::Ui,
    render: &mut ResMut<RenderSettings>,
    lighting: &mut ResMut<LightingSettings>,
    scenery: &mut ResMut<ScenerySettings>,
) {
    use gradiance_domain::theme::Look;
    widgets::section_header(ui, "Look");
    let current = render.look;
    ui.horizontal(|ui| {
        for look in Look::ALL {
            if ui
                .selectable_label(current == look, look.theme().label)
                .on_hover_text("apply this look's palette tone, ink, lighting and toon preset")
                .clicked()
                && current != look
            {
                **render = look.render();
                **lighting = look.lighting();
                **scenery = look.scenery();
            }
        }
    });
}

/// The toon ramp (brief 4b-6): the gradient picker for the falloff. Scene
/// content: written in place like the rest of the tab, a settled edit is one
/// undo step (`commit_settings_edits`); reset restores the look's ramp.
fn ramp_section(ui: &mut egui::Ui, render: &mut RenderSettings) {
    widgets::section_header(ui, "Ramp");
    let theme = render.look.theme();
    // Three palette fills through the ramp: every body shares the falloff.
    let fills = [0, 4, 8].map(|slot| theme.palette[slot].color);
    let default = render.look.ramp();
    crate::ramp_editor::ramp_editor(ui, "render-ramp", &mut render.ramp, &fills, &default);
}

/// The Interface tab: the chrome preset (brief U-7). Workstation config:
/// written directly, never undone, not saved with the scene (the same
/// write `chrome.apply` makes).
fn interface_tab(
    ui: &mut egui::Ui,
    chrome: &mut ResMut<gradiance_domain::settings::ChromeSettings>,
) {
    use gradiance_domain::settings::Chrome;
    widgets::section_header(ui, "Chrome");
    let current = chrome.chrome;
    for preset in Chrome::ALL {
        if ui
            .selectable_label(current == preset, preset.label())
            .on_hover_text("draw the panels with this preset (yours, not the scene's)")
            .clicked()
            && current != preset
        {
            chrome.chrome = preset;
        }
    }
    widgets::hint(
        ui,
        "The selection highlight follows the scene look's accent (Rendering > Look).",
    );
}

/// The Interface tab's units (brief 5-3): how every field shows angles and
/// lengths. Workstation config: written directly, never undone, not saved
/// with the scene (the same write `units.display` makes). Display only:
/// values stay SI, and a value typed with its unit (`25 cm`, `90 deg`)
/// converts whatever is shown.
fn units_section(ui: &mut egui::Ui, units: &mut ResMut<gradiance_domain::settings::UnitSettings>) {
    widgets::section_header(ui, "Units");
    let current = units.prefs;
    egui::Grid::new("units-display")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("angles");
            ui.horizontal(|ui| {
                for (angle, label) in [
                    (AngleUnit::Degrees, "degrees"),
                    (AngleUnit::Radians, "radians"),
                ] {
                    if ui
                        .selectable_label(current.angle == angle, label)
                        .on_hover_text(
                            "how angle fields and readouts show (yours, not the scene's)",
                        )
                        .clicked()
                        && current.angle != angle
                    {
                        units.prefs.angle = angle;
                    }
                }
            });
            ui.end_row();
            ui.label("lengths");
            ui.horizontal(|ui| {
                for length in LengthUnit::ALL {
                    if ui
                        .selectable_label(current.length == length, length.name())
                        .on_hover_text(
                            "how length fields and readouts show (yours, not the scene's)",
                        )
                        .clicked()
                        && current.length != length
                    {
                        units.prefs.length = length;
                    }
                }
            });
            ui.end_row();
        });
    widgets::hint(
        ui,
        "Type a value with its unit in any field (25 cm, 90 deg, 2 kN, 2*pi); a bare number is in the unit shown.",
    );
}

/// `SimSettings` fields [`simulation_tab`] draws by hand; any field added
/// later still appears, through the reflect grid below them.
pub(crate) const SIM_HAND_DRAWN: &[&str] = &[
    "gravity",
    "speed",
    "substeps",
    "plane_friction",
    "timestep_hz",
    "air_density",
    "drag_coefficient",
    "linear_drag",
    "wind",
];

/// The Simulation tab: the gravity-direction dial plus scrub-drag rows
/// (`labelled_drag` — the config seam; a settled edit becomes one undo step
/// through `commit_settings_edits`, so no gesture bookkeeping here).
fn simulation_tab(ui: &mut egui::Ui, sim: &mut SimSettings) {
    widgets::section_header(ui, "Gravity");
    let (mut angle, mut magnitude) = sim.gravity_polar();
    let defaults = SimSettings::default();
    let (default_angle, default_magnitude) = defaults.gravity_polar();
    let mut gravity_changed = false;
    ui.horizontal(|ui| {
        gravity_changed |= gravity_dial(ui, &mut angle, magnitude);
        egui::Grid::new("sim-gravity")
            .num_columns(2)
            .show(ui, |ui| {
                // The polar angle is kept in degrees; the field shows the
                // preferred angle unit (brief 5-3).
                gravity_changed |= widgets::labelled_drag(
                    ui,
                    "direction",
                    "counter-clockwise from +x; straight down is -90 degrees",
                    &mut angle,
                    widgets::config_field(default_angle, 1.0)
                        .range(-180.0..=180.0)
                        .stored(DisplayUnit::DEGREES),
                );
                gravity_changed |= widgets::labelled_drag(
                    ui,
                    "strength",
                    "gravity magnitude; 0 = weightless",
                    &mut magnitude,
                    widgets::config_field(default_magnitude, 0.1)
                        .range(0.0..=1000.0)
                        .unit(Dimension::Acceleration),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "x {:.2}  y {:.2}",
                        sim.gravity.value().x,
                        sim.gravity.value().y
                    ))
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
                "simulation speed multiplier; 1 = real time",
                &mut sim.speed,
                widgets::config_field(defaults.speed, 0.01).range(0.0..=10.0),
            );
            let substeps = widgets::config_field(defaults.substeps as f32, 0.2).range(1.0..=64.0);
            widgets::labelled_drag_int(
                ui,
                "substeps",
                "solver substeps per physics step: stiffer, more accurate, slower",
                &mut sim.substeps,
                substeps,
            );
            widgets::labelled_drag(
                ui,
                "timestep",
                "physics steps per second",
                &mut sim.timestep_hz.0,
                widgets::config_field(defaults.timestep_hz.0, 1.0)
                    .range(10.0..=480.0)
                    .unit(Dimension::Frequency),
            );
            widgets::labelled_drag(
                ui,
                "plane friction",
                "Coulomb friction against the back plane (top-down mode); 0 = off",
                &mut sim.plane_friction,
                widgets::config_field(defaults.plane_friction, 0.01).range(0.0..=5.0),
            );
        });
    ui.separator();
    air_section(ui, sim, &defaults);
    reflect_grid_except(ui, egui::Id::new("sim"), sim, &[], SIM_HAND_DRAWN);
}

/// The Air section (brief N-4): drag and wind, config-seam rows like the
/// solver's. Wind acts only through drag, so its rows say so.
fn air_section(ui: &mut egui::Ui, sim: &mut SimSettings, defaults: &SimSettings) {
    widgets::section_header(ui, "Air");
    egui::Grid::new("sim-air").num_columns(2).show(ui, |ui| {
        widgets::labelled_drag(
            ui,
            "drag",
            "quadratic drag coefficient Cd: 1/2 rho Cd w v^2 against the motion, w the width across it; 0 = off",
            &mut sim.drag_coefficient,
            widgets::config_field(defaults.drag_coefficient, 0.01).range(0.0..=5.0),
        );
        widgets::labelled_drag(
            ui,
            "linear drag",
            "viscous drag in N s/m^2: c w v against the motion; 0 = off",
            &mut sim.linear_drag,
            widgets::config_field(defaults.linear_drag, 0.01).range(0.0..=100.0),
        );
        widgets::labelled_drag(
            ui,
            "air density",
            "the air's density over the world's 1 m depth; 1.2 is air at sea level",
            &mut sim.air_density.0,
            widgets::config_field(defaults.air_density.0, 0.01)
                .range(0.0..=1000.0)
                .unit(Dimension::Density),
        );
        widgets::labelled_drag(
            ui,
            "wind x",
            "the air's velocity; it pushes only through drag",
            &mut sim.wind.0.x,
            widgets::config_field(defaults.wind.0.x, 0.1)
                .range(-100.0..=100.0)
                .unit(Dimension::Velocity),
        );
        widgets::labelled_drag(
            ui,
            "wind y",
            "the air's velocity; it pushes only through drag",
            &mut sim.wind.0.y,
            widgets::config_field(defaults.wind.0.y, 0.1)
                .range(-100.0..=100.0)
                .unit(Dimension::Velocity),
        );
    });
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
    let tokens = crate::theme::tokens(ui.ctx());
    painter.circle_stroke(center, RADIUS, egui::Stroke::new(1.0, tokens.dial));
    let a = angle_deg.to_radians();
    let dir = egui::vec2(a.cos(), -a.sin());
    // Weightless: a dot, not an arrow — there is no direction to show.
    let color = if magnitude > f32::EPSILON {
        ui.visuals().strong_text_color()
    } else {
        tokens.muted
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
    let defaults = CameraSettings::default();
    egui::Grid::new("camera").num_columns(2).show(ui, |ui| {
        changed |= widgets::labelled_drag(
            ui,
            "zoom step",
            "zoom multiplier per scroll notch: closer to 1 is gentler",
            &mut camera.zoom_step,
            widgets::config_field(defaults.zoom_step, 0.002).range(lo..=hi),
        );
        changed |= widgets::labelled_drag(
            ui,
            "pan speed (px/s)",
            "arrow-key pan speed in screen pixels per second",
            &mut camera.pan_speed,
            widgets::config_field(defaults.pan_speed, 5.0).range(0.0..=5000.0),
        );
        changed |= widgets::labelled_drag(
            ui,
            &format!("orbit speed ({}/px)", glyph::DEGREE),
            "middle-drag orbit sensitivity",
            &mut camera.orbit_speed_deg,
            widgets::config_field(defaults.orbit_speed_deg, 0.005).range(0.0..=5.0),
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
        &[],
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
        &[("rotation_step_deg", DisplayUnit::DEGREES)],
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
    let light_default = gradiance_domain::settings::KeyLightSettings::default();
    let defaults = LightingSettings::default();
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
                    changed |= widgets::config_field(light_default.illuminance, 100.0)
                        .show(ui, &mut light.illuminance)
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
            color: gradiance_domain::theme::FILL_LIGHT,
            illuminance: 900.0,
            ..KeyLightSettings::default()
        });
        changed = true;
    }

    ui.separator();
    widgets::section_header(ui, "Scene");
    ui.horizontal(|ui| {
        ui.label("ambient");
        changed |= widgets::config_field(defaults.ambient, 5.0)
            .show(ui, &mut settings.ambient)
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
        changed |= widgets::config_field(defaults.shadow_distance.0, 2.0)
            .unit(Dimension::Length)
            .show(ui, &mut settings.shadow_distance.0)
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
        &[("perspective_deg", DisplayUnit::DEGREES)],
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
    let tokens = crate::theme::tokens(ui.ctx());
    painter.circle_stroke(center, RADIUS, egui::Stroke::new(1.0, tokens.dial));
    painter.circle_stroke(center, RADIUS * 0.5, egui::Stroke::new(0.5, tokens.muted));
    let azimuth = light.azimuth_deg.to_radians();
    let r = (1.0 - (light.elevation_deg / 90.0).clamp(0.0, 1.0)) * RADIUS;
    let handle = center + egui::vec2(azimuth.cos(), -azimuth.sin()) * r;
    painter.line_segment([center, handle], egui::Stroke::new(1.0, tokens.dial));
    let tint = crate::theme::color32(light.color.with_alpha(1.0));
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
                frequency,
                damping_ratio,
                range,
            } => {
                let clamp = range.map_or(String::new(), |[lo, hi]| {
                    format!(" [{:.2},{:.2}]", lo.value(), hi.value())
                });
                format!(
                    "Spring rest={:.2} f={:.2}Hz zeta={:.2}{clamp}",
                    rest_length.value(),
                    frequency.value(),
                    damping_ratio
                )
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
