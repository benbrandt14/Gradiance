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
            // Fully matte: with any PBR specular (roughness < 1 or
            // nonzero reflectance) the banded luminance is view-dependent
            // and every front face — all sharing one normal — jumps bands
            // *simultaneously* as the camera tilts, snapping the whole
            // scene's color. The stylized highlight is the shader's
            // quantized glint instead.
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            metallic: 0.0,
            // Double-sided: the extruded front cap follows lyon's fill
            // winding, which faces away from the camera and would be
            // back-face culled otherwise (the "front face not visible"
            // bug). Bevy flips the normal for back faces so lighting
            // stays correct on both sides.
            double_sided: true,
            cull_mode: None,
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
    changed: Query<
        (Entity, &Appearance, &crate::domain::shape::ShapeDef),
        (With<Body>, Changed<Appearance>),
    >,
) {
    for (entity, appearance, shape) in &changed {
        // Half-plane grounds render through `render::ground`'s material.
        if matches!(shape, crate::domain::shape::ShapeDef::HalfPlane) {
            continue;
        }
        let handle = materials.add(body_material(appearance, &settings));
        commands.entity(entity).insert(MeshMaterial3d(handle));
    }
}
