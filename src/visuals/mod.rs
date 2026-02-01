//! Visual effects and rendering configuration.
//!
//! Handles global rendering settings (Bloom, Shadows, Toon Shading) and custom materials.

use crate::geometry::extrusion::ExtrudableShape;
use crate::prelude::*;
use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::experimental::taa::TemporalAntiAliasing;

mod gizmos;
use bevy::pbr::{
    MaterialPipeline, MaterialPipelineKey, ScreenSpaceAmbientOcclusion, ScreenSpaceReflections,
};
use bevy::reflect::TypePath;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, ShaderType, SpecializedMeshPipelineError,
};

/// Global rendering settings accessible via UI.
#[derive(Resource, Reflect, Debug)]
#[reflect(Resource)]
pub struct RenderSettings {
    /// Enable Bloom post-processing.
    pub bloom_enabled: bool,
    /// Intensity of the Bloom effect.
    pub bloom_intensity: f32,
    /// Enable SSAO.
    pub ssao_enabled: bool,
    /// Enable TAA.
    pub taa_enabled: bool,
    /// Enable SSR.
    pub ssr_enabled: bool,
    /// Enable Point Lights.
    pub point_lights_enabled: bool,
    /// Enable shadows for the main directional light.
    pub shadows_enabled: bool,
    /// Enable Toon Shading mode (swaps StandardMaterial for ToonMaterial).
    pub toon_mode: bool,
    /// The clear color (background color) of the camera.
    pub clear_color: Color,
    /// The color of the main directional light (Sun).
    pub sun_color: Color,
    /// The ambient light color.
    pub ambient_color: Color,
    /// Number of shading steps (bands) for the Toon Shader.
    pub toon_steps: u32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            bloom_enabled: true,
            bloom_intensity: 0.15,
            ssao_enabled: false,
            taa_enabled: false,
            ssr_enabled: false,
            point_lights_enabled: false,
            shadows_enabled: true,
            toon_mode: false,
            clear_color: Color::BLACK,
            sun_color: Color::WHITE,
            ambient_color: Color::srgb(0.5, 0.5, 0.5),
            toon_steps: 4,
        }
    }
}

/// Marker component for the main camera used by the Toon Shader.
#[derive(Component)]
pub struct ToonShaderMainCamera;

/// Marker component for the sun (directional light) used by the Toon Shader.
#[derive(Component)]
pub struct ToonShaderSun;

/// Marker for the scene point light.
#[derive(Component)]
pub struct ScenePointLight;

/// A custom material for Toon / Cel Shading.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[uniform(0, ToonMaterialUniform)]
pub struct ToonMaterial {
    /// Base color of the material.
    pub color: LinearRgba,
    /// Direction of the sun (light source).
    pub sun_dir: Vec3,
    /// Color of the sun.
    pub sun_color: LinearRgba,
    /// Position of the camera.
    pub camera_pos: Vec3,
    /// Ambient light color.
    pub ambient_color: LinearRgba,
    /// Number of shading steps (bands).
    pub steps: u32,
    /// Base color texture (optional).
    #[texture(1)]
    #[sampler(2)]
    pub base_color_texture: Option<Handle<Image>>,
}

/// Uniform struct for ToonMaterial.
#[derive(ShaderType, Clone, Debug)]
pub struct ToonMaterialUniform {
    /// Base color.
    pub color: LinearRgba,
// [dunnage] WARN: function `check` is never used
    /// Sun direction.
    pub sun_dir: Vec3,
    /// Sun color.
// [dunnage] WARN: function `check` is never used
    pub sun_color: LinearRgba,
    /// Camera position.
    pub camera_pos: Vec3,
// [dunnage] WARN: function `check` is never used
    /// Ambient color.
    pub ambient_color: LinearRgba,
    /// Shading steps.
// [dunnage] WARN: function `check` is never used
    pub steps: u32,
}

// [dunnage] WARN: function `check` is never used
impl From<&ToonMaterial> for ToonMaterialUniform {
    fn from(m: &ToonMaterial) -> Self {
        Self {
// [dunnage] WARN: function `check` is never used
            color: m.color,
            sun_dir: m.sun_dir,
            sun_color: m.sun_color,
            camera_pos: m.camera_pos,
            ambient_color: m.ambient_color,
            steps: m.steps,
        }
    }
}

impl Material for ToonMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/toon.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Plugin for visual effects and rendering configuration.
pub struct GradianceVisualsPlugin;

impl Plugin for GradianceVisualsPlugin {
    fn build(&self, app: &mut App) {
        // Also consider using `Changed<RenderSettings>` for `apply_render_settings` to avoid per-frame overhead.
        app.add_plugins(MaterialPlugin::<ToonMaterial>::default())
            .init_resource::<RenderSettings>()
            .init_resource::<AmbientLight>()
            .register_type::<RenderSettings>()
            .add_systems(
                Update,
                (
                    tag_main_camera_and_sun,
                    apply_render_settings,
                    update_toon_shader,
                    sync_to_toon_material,
                    sync_to_standard_material,
                    gizmos::draw_joint_gizmos,
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// Ensures the main camera and sun have the required marker components.
fn tag_main_camera_and_sun(
    mut commands: Commands,
    camera_query: Query<Entity, (With<Camera3d>, Without<ToonShaderMainCamera>)>,
    sun_query: Query<Entity, (With<DirectionalLight>, Without<ToonShaderSun>)>,
) {
    for entity in &camera_query {
        commands.entity(entity).insert(ToonShaderMainCamera);
    }
    for entity in &sun_query {
        commands.entity(entity).insert(ToonShaderSun);
    }
}

/// Updates `ToonMaterial` uniforms based on the scene's camera and sun.
fn update_toon_shader(
    main_cam: Query<&GlobalTransform, With<ToonShaderMainCamera>>,
    sun: Query<(&GlobalTransform, &DirectionalLight), With<ToonShaderSun>>,
    ambient_light: Option<Res<AmbientLight>>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
) {
    let camera_pos = main_cam
        .get_single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    let (sun_dir, sun_color) = sun
        .get_single()
        .map(|(t, l)| (t.back(), l.color))
        .unwrap_or((Dir3::Y, Color::WHITE));

    let ambient = ambient_light.map(|l| l.color).unwrap_or(Color::BLACK);

    for (_, toon_mat) in toon_materials.iter_mut() {
        toon_mat.camera_pos = camera_pos;
        toon_mat.sun_dir = *sun_dir;
        toon_mat.sun_color = LinearRgba::from(sun_color);
        toon_mat.ambient_color = LinearRgba::from(ambient);
    }
}

/// Syncs `RenderSettings` to Bevy's rendering components/resources.
// TODO: Refactor to reduce arguments (e.g. use a SystemParam struct)
fn apply_render_settings(
    settings: Res<RenderSettings>,
    mut bloom_query: Query<&mut Bloom>,
    mut light_query: Query<&mut DirectionalLight>,
    point_light_query: Query<(Entity, &mut PointLight), With<ScenePointLight>>,
    mut ambient_light: ResMut<AmbientLight>,
    mut clear_color: ResMut<ClearColor>,
// [dunnage] WARN: this function has too many arguments (13/7)
    mut commands: Commands,
    mut camera_query: Query<(Entity, Option<&mut Msaa>), With<Camera3d>>,
    bloom_removals: Query<Entity, (With<Camera3d>, With<Bloom>)>,
    ssao_removals: Query<Entity, (With<Camera3d>, With<ScreenSpaceAmbientOcclusion>)>,
    taa_removals: Query<Entity, (With<Camera3d>, With<TemporalAntiAliasing>)>,
    ssr_removals: Query<Entity, (With<Camera3d>, With<ScreenSpaceReflections>)>,
    mut toon_materials: ResMut<Assets<ToonMaterial>>,
) {
    // Sync Clear Color
    if clear_color.0 != settings.clear_color {
        clear_color.0 = settings.clear_color;
    }

    // Camera Post-Processing
    for (entity, msaa_opt) in &mut camera_query {
        let mut entity_cmds = commands.entity(entity);

        // Bloom
        if settings.bloom_enabled {
            if let Ok(mut bloom) = bloom_query.get_mut(entity) {
                if bloom.intensity != settings.bloom_intensity {
                    bloom.intensity = settings.bloom_intensity;
                }
            } else {
                entity_cmds.insert(Bloom {
                    intensity: settings.bloom_intensity,
                    ..default()
                });
            }
        } else if bloom_removals.contains(entity) {
            entity_cmds.remove::<Bloom>();
        }

        // SSAO
        if settings.ssao_enabled {
            entity_cmds.insert(ScreenSpaceAmbientOcclusion::default());
        } else if ssao_removals.contains(entity) {
            entity_cmds.remove::<ScreenSpaceAmbientOcclusion>();
        }

        // TAA
        if settings.taa_enabled {
            if let Some(mut msaa) = msaa_opt
                && *msaa != Msaa::Off {
                    *msaa = Msaa::Off;
                }
            entity_cmds.insert(TemporalAntiAliasing::default());
        } else if taa_removals.contains(entity) {
            entity_cmds.remove::<TemporalAntiAliasing>();
            // Optional: Restore MSAA here if desired, but default to Off is safe
        }

        // SSR
        if settings.ssr_enabled {
            entity_cmds.insert(ScreenSpaceReflections::default());
        } else if ssr_removals.contains(entity) {
            entity_cmds.remove::<ScreenSpaceReflections>();
        }
    }

    // Sync Shadows and Sun Color
    for mut light in &mut light_query {
        if light.shadows_enabled != settings.shadows_enabled {
            light.shadows_enabled = settings.shadows_enabled;
        }
        if light.color != settings.sun_color {
            light.color = settings.sun_color;
        }
    }

    // Point Light Management
    if settings.point_lights_enabled {
        if point_light_query.iter().next().is_none() {
            // Spawn if missing
            commands.spawn((
                Name::new("ScenePointLight"),
                PointLight {
                    intensity: 200_000.0,
                    range: 100.0,
                    shadows_enabled: true,
                    color: Color::srgb(1.0, 0.8, 0.6), // Warm light
                    ..default()
                },
                Transform::from_xyz(20.0, 20.0, 30.0),
                ScenePointLight,
            ));
        }
    } else {
        // Despawn if present
        for (e, _) in &point_light_query {
            commands.entity(e).despawn();
        }
    }

    // Sync Ambient Light
    if ambient_light.color != settings.ambient_color {
        ambient_light.color = settings.ambient_color;
    }

    // Sync Toon Steps
    for (_, mat) in toon_materials.iter_mut() {
        if mat.steps != settings.toon_steps {
            mat.steps = settings.toon_steps;
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
        let color = materials
            .get(&handle.0)
            .map(|m| m.base_color)
            .unwrap_or(Color::WHITE);

        // Create equivalent ToonMaterial
        let toon_material = ToonMaterial {
            color: LinearRgba::from(color),
            steps: settings.toon_steps,
            sun_dir: Vec3::Y,             // Will be updated by update_toon_shader
            sun_color: LinearRgba::WHITE, // Will be updated by update_toon_shader
            camera_pos: Vec3::ZERO,       // Will be updated by update_toon_shader
            ambient_color: LinearRgba::BLACK, // Will be updated by update_toon_shader
            base_color_texture: None, // StandardMaterial texture handling omitted for simplicity
        };

        let new_handle = toon_materials.add(toon_material);

        // Swap components
        commands
            .entity(entity)
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
        let color = toon_materials
            .get(&handle.0)
            .map(|m| Color::from(m.color))
            .unwrap_or(Color::WHITE);

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
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<ToonMaterial>>()
            .insert(MeshMaterial3d(new_handle));
    }
}
