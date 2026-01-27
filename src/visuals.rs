//! Visual effects and rendering configuration.
//!
//! Handles global rendering settings (Bloom, Shadows, Toon Shading) and custom materials.

use crate::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use bevy::reflect::TypePath;
use bevy::core_pipeline::bloom::Bloom;
use crate::geometry::extrusion::ExtrudableShape;

/// Global rendering settings accessible via UI.
#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct RenderSettings {
    /// Enable Bloom post-processing.
    pub bloom_enabled: bool,
    /// Intensity of the Bloom effect.
    pub bloom_intensity: f32,
    /// Enable shadows for the main directional light.
    pub shadows_enabled: bool,
    /// Enable Toon Shading mode (swaps StandardMaterial for ToonMaterial).
    pub toon_mode: bool,
    /// The clear color (background color) of the camera.
    pub clear_color: Color,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            bloom_enabled: true,
            bloom_intensity: 0.15,
            shadows_enabled: true,
            toon_mode: false,
            clear_color: Color::BLACK,
        }
    }
}

/// A custom material for Toon / Cel Shading.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct ToonMaterial {
    /// Base color of the material.
    #[uniform(0)]
    pub color: LinearRgba,
    /// Number of shading steps (bands).
    #[uniform(0)]
    pub steps: u32,
    /// Width of the border/outline (if implemented).
    #[uniform(0)]
    pub border_width: f32,
}

impl Material for ToonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/toon.wgsl".into()
    }
}

/// Plugin for visual effects and rendering configuration.
pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ToonMaterial>::default())
           .init_resource::<RenderSettings>()
           .register_type::<RenderSettings>()
           .add_systems(Update, (
               apply_render_settings,
               sync_to_toon_material,
               sync_to_standard_material
            ));
    }
}

/// Syncs `RenderSettings` to Bevy's rendering components/resources.
fn apply_render_settings(
    settings: Res<RenderSettings>,
    mut bloom_query: Query<&mut Bloom>,
    mut light_query: Query<&mut DirectionalLight>,
    mut clear_color: ResMut<ClearColor>,
    mut commands: Commands,
    camera_query: Query<Entity, (With<Camera3d>, Without<Bloom>)>,
    bloom_removals: Query<Entity, (With<Camera3d>, With<Bloom>)>,
) {
    // Sync Clear Color
    if clear_color.0 != settings.clear_color {
        clear_color.0 = settings.clear_color;
    }

    // Sync Bloom
    if settings.bloom_enabled {
        // Update existing Bloom
        for mut bloom in &mut bloom_query {
            if bloom.intensity != settings.bloom_intensity {
                bloom.intensity = settings.bloom_intensity;
            }
        }
        // Add Bloom if missing
        for entity in &camera_query {
            commands.entity(entity).insert(Bloom {
                intensity: settings.bloom_intensity,
                ..default()
            });
        }
    } else {
        // Remove Bloom if present
        for entity in &bloom_removals {
            commands.entity(entity).remove::<Bloom>();
        }
    }

    // Sync Shadows
    for mut light in &mut light_query {
        if light.shadows_enabled != settings.shadows_enabled {
            light.shadows_enabled = settings.shadows_enabled;
        }
    }
}

/// Switches `ExtrudableShape` entities to `ToonMaterial` when `toon_mode` is enabled.
fn sync_to_toon_material(
    mut commands: Commands,
    settings: Res<RenderSettings>,
    // Identify shapes that currently have StandardMaterial but should be Toon
    query: Query<(Entity, &MeshMaterial3d<StandardMaterial>), With<ExtrudableShape>>,
    materials: Res<Assets<StandardMaterial>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
) {
    if !settings.toon_mode {
        return;
    }

    for (entity, handle) in &query {
        // Extract color from current material
        let color = materials.get(&handle.0).map(|m| m.base_color).unwrap_or(Color::WHITE);

        // Create equivalent ToonMaterial
        let toon_material = ToonMaterial {
            color: LinearRgba::from(color),
            steps: 4,
            border_width: 0.05,
        };

        let new_handle = toon_materials.add(toon_material);

        // Swap components
        commands.entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(new_handle));
    }
}

/// Switches `ExtrudableShape` entities to `StandardMaterial` when `toon_mode` is disabled.
fn sync_to_standard_material(
    mut commands: Commands,
    settings: Res<RenderSettings>,
    // Identify shapes that currently have ToonMaterial but should be Standard
    query: Query<(Entity, &MeshMaterial3d<ToonMaterial>), With<ExtrudableShape>>,
    toon_materials: Res<Assets<ToonMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if settings.toon_mode {
        return;
    }

    for (entity, handle) in &query {
        // Extract color from current material
        let color = toon_materials.get(&handle.0).map(|m| Color::from(m.color)).unwrap_or(Color::WHITE);

        // Create equivalent StandardMaterial
        let standard_material = StandardMaterial {
            base_color: color,
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.5,
            ..default()
        };

        let new_handle = materials.add(standard_material);

        // Swap components
        commands.entity(entity)
            .remove::<MeshMaterial3d<ToonMaterial>>()
            .insert(MeshMaterial3d(new_handle));
    }
}
