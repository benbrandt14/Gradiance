//! Fixed scene furniture derived from settings: the key light and the
//! shadow-catching back plane.
//!
//! Both were hard-coded spawns; they are now synced from the Config-seam
//! resources [`LightingSettings`] / [`ScenerySettings`] via change detection,
//! so the Lighting tab (and scripts, through reflection) drive them without
//! touching entities. The back plane additionally tracks the deepest
//! occupied layer each frame so it always sits `back_offset` behind the
//! scene.

use crate::plane::{InfinitePlaneMaterial, PLANE_EXTENT, plane_material};
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, GlobalAmbientLight};
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::Rgba;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::settings::{LightingSettings, ScenerySettings};

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
    mut materials: ResMut<Assets<InfinitePlaneMaterial>>,
    clear: Res<ClearColor>,
) {
    // Key lights are spawned by `apply_lighting` from the settings list.
    // The back plane is what *catches cast shadows* — without it every drop
    // shadow falls into the void and the scene reads as flat, unlit 2D.
    // Render-only (no collider, no StableId, never saved); same infinite-
    // plane material as half-plane grounds, differing only in orientation.
    commands.spawn((
        BackPlane,
        Mesh3d(meshes.add(Mesh::from(Rectangle::new(
            PLANE_EXTENT * 2.0,
            PLANE_EXTENT * 2.0,
        )))),
        MeshMaterial3d(materials.add(plane_material(Color::WHITE, Vec3::Z, 0.0, clear.0, 0.0))),
        Transform::default(),
    ));
}

/// Applies [`LightingSettings`]: rebuilds the key-light set, ambient,
/// shadow quality, and camera AO. Runs change-detected (and once at
/// startup, since the resource counts as added).
///
/// Shadow crispness: prisms want *hard* shadows, so each light gets a
/// single high-resolution cascade sized by `shadow_distance` — the bevy
/// default fits cascades to the view frustum, which both softens edges
/// (sparse texels) and clips shadows as the view orbits.
pub fn apply_lighting(
    settings: Res<LightingSettings>,
    mut ambient: ResMut<GlobalAmbientLight>,
    existing: Query<Entity, With<KeyLight>>,
    cameras: Query<Entity, With<Camera3d>>,
    mut commands: Commands,
) {
    // Rebuild the light set (a handful of entities, change-detected only).
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    // Cascade near/far are absolute world metres (SI): sized to the scene so
    // the shadow map's texels stay small relative to metre-scale bodies —
    // an oversized cascade spreads texels out and acnes the catcher plane.
    let cascade = CascadeShadowConfigBuilder {
        num_cascades: 1,
        minimum_distance: 0.01,
        maximum_distance: settings.shadow_distance.max(1.0),
        ..default()
    }
    .build();
    for light in &settings.lights {
        let azimuth = light.azimuth_deg.to_radians();
        let elevation = light.elevation_deg.clamp(1.0, 89.0).to_radians();
        // Position on the from-direction ray (distance is arbitrary for a
        // directional light; only orientation matters).
        let from = Vec3::new(
            elevation.cos() * azimuth.cos(),
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
        ) * 1_000.0;
        commands.spawn((
            KeyLight,
            DirectionalLight {
                color: color_of(light.color),
                illuminance: light.illuminance.max(0.0),
                shadow_maps_enabled: light.shadows,
                contact_shadows_enabled: settings.contact_shadows,
                ..default()
            },
            cascade.clone(),
            Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y),
        ));
    }
    commands.insert_resource(DirectionalLightShadowMap {
        size: (settings.shadow_map_size.max(256) as usize).next_power_of_two(),
    });
    ambient.color = Color::WHITE;
    ambient.brightness = settings.ambient.max(0.0);
    // SSAO wants Msaa off plus depth/normal prepasses; screen-space contact
    // shadows raymarch the depth buffer, so they need the depth prepass too.
    for camera in &cameras {
        let mut entity = commands.entity(camera);
        if settings.ssao {
            entity.insert((
                ScreenSpaceAmbientOcclusion::default(),
                DepthPrepass,
                NormalPrepass,
                Msaa::Off,
            ));
        } else {
            entity
                .remove::<(ScreenSpaceAmbientOcclusion, NormalPrepass)>()
                .insert(Msaa::default());
            if settings.contact_shadows {
                entity.insert(DepthPrepass);
            } else {
                entity.remove::<DepthPrepass>();
            }
        }
    }
}

/// Applies [`ScenerySettings`] color/visibility to the back plane
/// (change-detected).
pub fn apply_scenery(
    settings: Res<ScenerySettings>,
    mut materials: ResMut<Assets<InfinitePlaneMaterial>>,
    mut planes: Query<(&MeshMaterial3d<InfinitePlaneMaterial>, &mut Visibility), With<BackPlane>>,
) {
    for (material, mut visibility) in &mut planes {
        if let Some(mut mat) = materials.get_mut(&material.0) {
            mat.base.base_color = color_of(settings.back_color);
        }
        *visibility = if settings.back_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Keeps the back plane `back_offset` behind the deepest occupied layer.
/// Runs every frame; writes transform + plane uniform only when the target
/// moves.
pub fn track_scene_depth(
    settings: Res<ScenerySettings>,
    bodies: Query<&DepthBand, With<Body>>,
    mut materials: ResMut<Assets<InfinitePlaneMaterial>>,
    mut planes: Query<(&MeshMaterial3d<InfinitePlaneMaterial>, &mut Transform), With<BackPlane>>,
) {
    let deepest_back = bodies
        .iter()
        .map(|band| -band.sanitized().far)
        .fold(-gradiance_core::constants::LAYER_HEIGHT, f32::min);
    let target = deepest_back - settings.back_offset.max(0.0);
    for (material, mut transform) in &mut planes {
        if (transform.translation.z - target).abs() > 1e-3 {
            transform.translation.z = target;
            if let Some(mut mat) = materials.get_mut(&material.0) {
                mat.extension.plane.w = target;
            }
        }
    }
}
