//! Tool for creating gears.
//!
//! Click and drag to define the radius (and thus tooth count) of a new gear.

use crate::input::commands::{CommandStack, SpawnGearCommand};
use crate::input::tools::utils::{DragStatus, handle_drag_input};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::geometry::gear::GearProfile;
use crate::ui::grid::GridSettings;
use bevy_egui::{egui, EguiContexts};

/// Plugin for the Gear Tool.
pub struct GearToolPlugin;

impl Plugin for GearToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GearToolData>();
        app.init_resource::<GearToolSettings>();
        app.add_systems(
            Update,
            (gear_tool_ui, gear_tool_update).run_if(in_state(ToolState::Gear)),
        );
        app.add_systems(OnExit(ToolState::Gear), gear_tool_reset);
    }
}

/// Settings for the Gear Tool.
#[derive(Resource)]
pub struct GearToolSettings {
    /// Module (size of teeth).
    pub module: f32,
    /// Pressure angle in degrees.
    pub pressure_angle: f32,
}

impl Default for GearToolSettings {
    fn default() -> Self {
        Self {
            module: 10.0,
            pressure_angle: 20.0,
        }
    }
}

#[derive(Resource, Default)]
struct GearToolData {
    drag_start: Option<Vec2>,
}

fn gear_tool_reset(mut data: ResMut<GearToolData>) {
    data.drag_start = None;
}

fn calculate_teeth(radius: f32, module: f32) -> u32 {
    let diameter = radius * 2.0;
    // Pitch Diameter = Z * m
    // Z = D / m
    let z = (diameter / module).round() as u32;
    z.max(3) // Minimum 3 teeth
}

fn gear_tool_ui(mut contexts: EguiContexts, mut settings: ResMut<GearToolSettings>) {
    egui::Window::new("Gear Settings")
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 10.0))
        .show(contexts.ctx_mut(), |ui| {
            ui.add(egui::Slider::new(&mut settings.module, 1.0..=50.0).text("Module"));
            ui.add(egui::Slider::new(&mut settings.pressure_angle, 14.5..=30.0).text("Pressure Angle"));
        });
}

fn gear_tool_update(
    mut commands: Commands,
    mut data: ResMut<GearToolData>,
    settings: Res<GearToolSettings>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut gizmos: Gizmos,
    contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
) {
    let Some(drag) = handle_drag_input(
        cursor_pos,
        mouse,
        grid_settings,
        contexts,
        &mut data.drag_start,
    ) else {
        return;
    };

    let radius = drag.start.distance(drag.current);
    let teeth = calculate_teeth(radius, settings.module);

    // Create temp profile for preview/spawn
    let profile = GearProfile::new(settings.module, teeth, settings.pressure_angle);
    let r_pitch = teeth as f32 * settings.module / 2.0;

    match drag.status {
        DragStatus::Dragging => {
            // Draw Pitch Circle
            gizmos.circle_2d(
                Isometry2d::from_translation(Vec2::new(drag.start.x, drag.start.y)),
                r_pitch,
                Color::srgb(1.0, 1.0, 1.0),
            );

            // Draw Gear Preview (low res)
            let points = profile.generate_points(2);
             if points.len() > 1 {
                 // Convert to 3D points for line_strip (z=0)
                 let pts: Vec<Vec3> = points.iter().map(|p| Vec3::new(p.x + drag.start.x, p.y + drag.start.y, 0.0)).collect();
                 let first = pts[0];
                 gizmos.linestrip(pts.into_iter().chain(std::iter::once(first)), Color::srgb(0.5, 0.5, 1.0));
             }

            // Draw line to cursor
            gizmos.line_2d(drag.start, drag.current, Color::srgb(0.5, 0.5, 0.5));

            // Show info text? (Gizmos don't support text easily, maybe debug text if available, or just rely on visual)
        }
        DragStatus::Finished => {
            if radius > settings.module { // Minimum size roughly
                let cmd = SpawnGearCommand {
                    position: drag.start,
                    profile,
                    entity: None,
                    pin_entity: None,
                };

                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }
        }
        _ => {}
    }
}
