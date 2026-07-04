//! Authored appearance → derived matte material.

use crate::domain::Body;
use crate::domain::appearance::{Appearance, Rgba};
use bevy::prelude::*;

/// Converts a domain color into a Bevy color (sRGB).
pub fn color_of(rgba: Rgba) -> Color {
    Color::srgba(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// The standard matte body material for a given appearance.
fn matte_material(appearance: &Appearance) -> StandardMaterial {
    StandardMaterial {
        base_color: color_of(appearance.fill),
        perceptual_roughness: 0.92,
        metallic: 0.0,
        ..default()
    }
}

/// (Re)builds a body's material when its appearance changes.
pub fn sync_body_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    changed: Query<(Entity, &Appearance), (With<Body>, Changed<Appearance>)>,
) {
    for (entity, appearance) in &changed {
        let handle = materials.add(matte_material(appearance));
        commands.entity(entity).insert(MeshMaterial3d(handle));
    }
}
