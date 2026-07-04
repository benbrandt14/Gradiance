//! Rendering: derived meshes/materials, scene lighting, and the grid.
//!
//! Default aesthetic: matte bodies under a shadow-casting directional
//! light. Everything here is derived state — meshes and materials are
//! rebuilt from authored components by `Changed<>`-driven sync systems and
//! never serialized.

pub mod extrude_sync;
pub mod grid;
pub mod material_sync;

use bevy::light::GlobalAmbientLight;
use bevy::prelude::*;

/// Installs cameras, lights, and the derived-visuals sync systems.
///
/// A no-op in headless apps (no render plugin present).
#[derive(Default)]
pub struct GradianceRenderPlugin;

impl Plugin for GradianceRenderPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<bevy::render::RenderPlugin>() {
            return;
        }
        app.insert_resource(GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 300.0,
            ..default()
        });
        app.add_systems(Startup, setup_scene);
        app.add_systems(
            PostUpdate,
            (
                extrude_sync::sync_body_meshes,
                material_sync::sync_body_materials,
            ),
        );
        app.add_systems(Update, grid::draw_grid);
    }
}

/// Spawns the editor camera and the shadow-casting key light.
fn setup_scene(mut commands: Commands) {
    // Orthographic 3D camera looking down −Z at the XY sandbox plane;
    // extruded bodies span z ∈ [-320, 0].
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection::default_3d()),
        Transform::from_xyz(0.0, 0.0, 600.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Angled key light so extrusion depth reads through cast shadows.
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-400.0, 700.0, 500.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
