//! Authored appearance → derived body material (toon-banded matte).

use crate::domain::Body;
use crate::domain::appearance::{Appearance, Rgba};
use crate::domain::settings::RenderSettings;
use crate::render::toon::{ToonExtension, ToonMaterial, params_of};
use bevy::prelude::*;

/// Converts a domain color into a Bevy color (sRGB).
pub fn color_of(rgba: Rgba) -> Color {
    Color::srgba(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// The standard body material: matte base, toon banding per settings.
fn body_material(appearance: &Appearance, settings: &RenderSettings) -> ToonMaterial {
    let fill = appearance.fill;
    ToonMaterial {
        base: StandardMaterial {
            base_color: color_of(fill),
            perceptual_roughness: 0.92,
            metallic: 0.0,
            // Emissive glow in the fill's hue (0 = matte).
            emissive: LinearRgba::new(
                fill.r * appearance.emissive,
                fill.g * appearance.emissive,
                fill.b * appearance.emissive,
                1.0,
            ),
            ..default()
        },
        extension: ToonExtension {
            params: params_of(settings),
        },
    }
}

/// (Re)builds a body's material when its appearance changes.
pub fn sync_body_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<ToonMaterial>>,
    settings: Res<RenderSettings>,
    changed: Query<(Entity, &Appearance), (With<Body>, Changed<Appearance>)>,
) {
    for (entity, appearance) in &changed {
        let handle = materials.add(body_material(appearance, &settings));
        commands.entity(entity).insert(MeshMaterial3d(handle));
    }
}
