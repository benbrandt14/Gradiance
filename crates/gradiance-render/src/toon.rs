//! The toon body material: `StandardMaterial` extended with banded
//! lighting.
//!
//! Extending (rather than replacing) the standard material keeps the
//! whole PBR feature set — cast shadow maps, clustered lights, ambient —
//! and quantizes the *lit result*, so the banding follows real
//! illumination. Band count and rim strength are shader parameters
//! driven by [`RenderSettings`]: toggling the style updates uniforms and
//! never rebuilds materials or meshes.

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use gradiance_domain::settings::RenderSettings;

/// Where the [`install`]ed `embedded_asset!` registration puts the shader:
/// `embedded://<crate underscore name>/<path from src/>`. Asserted against
/// the macro's own computation in this file's tests.
const TOON_SHADER_PATH: &str = "embedded://gradiance_render/toon.wgsl";

/// The material every body renders with.
pub type ToonMaterial = ExtendedMaterial<StandardMaterial, ToonExtension>;

/// Banding parameters appended to the standard material's bind group.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct ToonExtension {
    /// `x` = band count (0 disables banding), `y` = rim strength,
    /// `z` = white point, `w` = quantized-specular strength.
    #[uniform(100)]
    pub params: Vec4,
}

impl MaterialExtension for ToonExtension {
    fn fragment_shader() -> ShaderRef {
        TOON_SHADER_PATH.into()
    }
}

/// The current shader parameters for `settings`.
pub fn params_of(settings: &RenderSettings) -> Vec4 {
    let bands = if settings.toon_enabled {
        settings.bands.max(1) as f32
    } else {
        0.0
    };
    Vec4::new(
        bands,
        settings.rim_strength,
        settings.white_point.max(0.05),
        settings.specular.max(0.0),
    )
}

/// Installs the toon material pipeline and its embedded shader.
pub fn install(app: &mut App) {
    embedded_asset!(app, "toon.wgsl");
    app.add_plugins(MaterialPlugin::<ToonMaterial>::default());
}

/// Pushes changed [`RenderSettings`] into every live body material.
pub fn apply_render_settings(
    settings: Res<RenderSettings>,
    mut materials: ResMut<Assets<ToonMaterial>>,
) {
    let params = params_of(&settings);
    let handles: Vec<_> = materials.ids().collect();
    for id in handles {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.params = params;
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exact sentinel values, not math
mod tests {
    use super::*;

    #[test]
    fn defaults_are_toon_with_four_bands() {
        let params = params_of(&RenderSettings::default());
        assert_eq!(params.x, 4.0);
        assert!(params.y > 0.0, "rim on by default");
    }

    #[test]
    fn disabling_toon_zeroes_the_band_count() {
        let params = params_of(&RenderSettings {
            toon_enabled: false,
            ..RenderSettings::default()
        });
        assert_eq!(params.x, 0.0, "bands = 0 bypasses banding in the shader");
    }

    #[test]
    fn band_count_never_reaches_zero_while_enabled() {
        let params = params_of(&RenderSettings {
            bands: 0,
            ..RenderSettings::default()
        });
        assert_eq!(params.x, 1.0, "clamped to one band, not divide-by-zero");
    }

    /// The `ShaderRef` string and the `embedded_asset!` registration must
    /// agree; the registration path is derived from `module_path!()` (crate
    /// name with underscores) + the file's location under `src/`, so a crate
    /// rename or file move silently breaks the reference at runtime unless
    /// this pins them together. Must live in this file (same `file!()` as
    /// the registration in [`super::install`]).
    #[test]
    fn shader_ref_matches_the_embedded_registration() {
        let registered = bevy::asset::embedded_path!("toon.wgsl");
        assert_eq!(
            format!("embedded://{}", registered.display()),
            TOON_SHADER_PATH
        );
    }
}
