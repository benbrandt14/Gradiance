//! Fixed scene furniture derived from settings: the key light and the
//! shadow-catching back plane.
//!
//! Both were hard-coded spawns; they are now synced from the Config-seam
//! resources [`LightingSettings`] / [`ScenerySettings`] via change detection,
//! so the Lighting tab (and scripts, through reflection) drive them without
//! touching entities. The back plane additionally tracks the deepest
//! occupied layer each frame so it always sits `back_offset` behind the
//! scene.

use crate::domain::Body;
use crate::domain::appearance::Rgba;
use crate::domain::layers::LayerMask32;
use crate::domain::settings::{LightingSettings, ScenerySettings};
use crate::geometry::extrusion::layer_z_range;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::light::GlobalAmbientLight;
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::prelude::*;

/// Marker: the shadow-catching back plane entity.
#[derive(Component)]
pub struct BackPlane;

/// Marker: the key directional light entity.
#[derive(Component)]
pub struct KeyLight;

fn color_of(c: Rgba) -> Color {
    Color::srgba(c.r, c.g, c.b, c.a)
}

/// Spawns the key light and the back plane (their parameters are applied by
/// the change-detected sync systems below on the first frame).
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        KeyLight,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::default(),
    ));
    // The back plane is what *catches cast shadows* — without it every drop
    // shadow falls into the void and the scene reads as flat, unlit 2D.
    // Render-only: no collider, no StableId, never saved.
    commands.spawn((
        BackPlane,
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(200_000.0, 200_000.0)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            perceptual_roughness: 1.0,
            metallic: 0.0,
            ..default()
        })),
        Transform::default(),
    ));
}

/// Applies [`LightingSettings`] to the key light, ambient, and camera AO.
/// Runs change-detected (and once at startup, since the resource counts as
/// added).
pub fn apply_lighting(
    settings: Res<LightingSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<KeyLight>>,
    cameras: Query<Entity, With<Camera3d>>,
    mut commands: Commands,
) {
    let azimuth = settings.azimuth_deg.to_radians();
    let elevation = settings.elevation_deg.clamp(1.0, 89.0).to_radians();
    // Light position on the from-direction ray (distance is arbitrary for a
    // directional light; only orientation matters).
    let from = Vec3::new(
        elevation.cos() * azimuth.cos(),
        elevation.cos() * azimuth.sin(),
        elevation.sin(),
    ) * 1_000.0;
    for (mut light, mut transform) in &mut lights {
        light.color = color_of(settings.color);
        light.illuminance = settings.illuminance.max(0.0);
        light.contact_shadows_enabled = settings.contact_shadows;
        *transform = Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.color = Color::WHITE;
    ambient.brightness = settings.ambient.max(0.0);
    // SSAO wants Msaa off plus depth/normal prepasses on the camera.
    for camera in &cameras {
        if settings.ssao {
            commands.entity(camera).insert((
                ScreenSpaceAmbientOcclusion::default(),
                DepthPrepass,
                NormalPrepass,
                Msaa::Off,
            ));
        } else {
            commands
                .entity(camera)
                .remove::<(ScreenSpaceAmbientOcclusion, DepthPrepass, NormalPrepass)>()
                .insert(Msaa::default());
        }
    }
}

/// Applies [`ScenerySettings`] color/visibility to the back plane
/// (change-detected).
pub fn apply_scenery(
    settings: Res<ScenerySettings>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut planes: Query<(&MeshMaterial3d<StandardMaterial>, &mut Visibility), With<BackPlane>>,
) {
    for (material, mut visibility) in &mut planes {
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base_color = color_of(settings.back_color);
        }
        *visibility = if settings.back_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Keeps the back plane `back_offset` behind the deepest occupied layer.
/// Runs every frame; writes the transform only when the target moves.
pub fn track_scene_depth(
    settings: Res<ScenerySettings>,
    bodies: Query<&LayerMask32, With<Body>>,
    mut planes: Query<&mut Transform, With<BackPlane>>,
) {
    let deepest_back = bodies
        .iter()
        .filter_map(LayerMask32::occupied_range)
        .map(|(min, max)| {
            let (z_front, depth) = layer_z_range(min, max);
            z_front - depth
        })
        .fold(-crate::core::constants::LAYER_HEIGHT, f32::min);
    let target = deepest_back - settings.back_offset.max(0.0);
    for mut transform in &mut planes {
        if (transform.translation.z - target).abs() > 1e-3 {
            transform.translation.z = target;
        }
    }
}
