//! The toon body material: `StandardMaterial` extended with a colour
//! ramp.
//!
//! Extending (rather than replacing) the standard material keeps the
//! whole PBR feature set — cast shadow maps, clustered lights, ambient —
//! and maps the *lit result* through the ramp, so the bands follow real
//! illumination. The ramp, dither and face relief are shader parameters
//! driven by [`RenderSettings`]: editing them updates uniforms and never
//! rebuilds materials or meshes.
//!
//! Brief 4b-6: lighting indexes a [`ColorRamp`](gradiance_domain::ramp::ColorRamp)
//! sampled once into [`ToonRamp`] (the frame loop indexes, never searches);
//! band edges dither in screen space; a front face is lit as a rounded body
//! from its [`FaceField`](gradiance_geometry::face_field::FaceField) (the
//! distance to its outline), uploaded per body as a small texture.

use bevy::asset::embedded_asset;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use gradiance_domain::ramp::{RAMP_TABLE, RampInterp};
use gradiance_domain::settings::RenderSettings;

/// Where the [`install`]ed `embedded_asset!` registration puts the shader:
/// `embedded://<crate underscore name>/<path from src/>`. Asserted against
/// the macro's own computation in this file's tests.
const TOON_SHADER_PATH: &str = "embedded://gradiance_render/toon.wesl";

/// The material every body renders with.
pub type ToonMaterial = ExtendedMaterial<StandardMaterial, ToonExtension>;

/// The sampled ramp and its dither, shared by bodies and planes (one
/// uniform, so a shadow on the back plane takes the bodies' tones).
#[derive(ShaderType, Debug, Clone)]
pub struct ToonRamp {
    /// [`ColorRamp::table`](gradiance_domain::ramp::ColorRamp::table):
    /// sRGB tones, `w` = 1.
    pub tones: [Vec4; RAMP_TABLE],
    /// `x` = 1 for linear interpolation (0 = constant), `y` = dither width
    /// in lighting levels (0 = hard edges), `z` = screen pixels per dither
    /// cell, `w` = 1 when toon shading is on (0 = smooth PBR).
    pub params: Vec4,
}

impl Default for ToonRamp {
    fn default() -> Self {
        ramp_of(&RenderSettings::default())
    }
}

/// Banding parameters appended to the standard material's bind group.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone, Default)]
pub struct ToonExtension {
    /// `x` = toon on (1) or smooth PBR (0), `y` = rim strength,
    /// `z` = white point, `w` = quantized-specular strength.
    #[uniform(100)]
    pub params: Vec4,
    /// Face shading: `x` = relief (how far the normal tilts at the
    /// outline; 0 = flat faces), `y` = the glint dome's slope, `z` = bevel
    /// (how far in the relief reaches, a fraction of the deepest depth),
    /// `w` = 1 when [`field`](Self::field) is set (else the 4b-5 dome over
    /// the face coordinates is the fallback).
    #[uniform(101)]
    pub face: Vec4,
    /// The ramp and the dither.
    #[uniform(102)]
    pub ramp: ToonRamp,
    /// This body's face field (`face_field::FaceField` as a linear
    /// `Rgba8Unorm` image over the face frame), when baked.
    #[texture(103)]
    #[sampler(104)]
    pub field: Option<Handle<Image>>,
    /// The body's image (brief S-13): `xyz` = the first row of the affine
    /// map from face coordinates to image coordinates, `w` = 1 when
    /// [`image`](Self::image) is set.
    #[uniform(105)]
    pub image_u: Vec4,
    /// The second row of that map; `w` = 1 for a raw (unlit) image.
    #[uniform(106)]
    pub image_v: Vec4,
    /// The body's image, when it has one (`texture_sync`).
    #[texture(107)]
    #[sampler(108)]
    pub image: Option<Handle<Image>>,
}

impl MaterialExtension for ToonExtension {
    fn fragment_shader() -> ShaderRef {
        TOON_SHADER_PATH.into()
    }
}

/// The current shader parameters for `settings`.
pub fn params_of(settings: &RenderSettings) -> Vec4 {
    Vec4::new(
        if settings.toon_enabled { 1.0 } else { 0.0 },
        settings.rim_strength,
        settings.white_point.max(0.05),
        settings.specular.max(0.0),
    )
}

/// The sampled ramp and dither for `settings` ([`ToonRamp`]).
pub fn ramp_of(settings: &RenderSettings) -> ToonRamp {
    let table = settings.ramp.table();
    let mut tones = [Vec4::ONE; RAMP_TABLE];
    for (tone, entry) in tones.iter_mut().zip(table) {
        *tone = Vec4::from_array(entry);
    }
    let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
    ToonRamp {
        tones,
        params: Vec4::new(
            if settings.ramp.interp == RampInterp::Linear {
                1.0
            } else {
                0.0
            },
            finite(settings.dither).clamp(0.0, 0.5),
            finite(settings.dither_scale).clamp(1.0, 16.0),
            if settings.toon_enabled { 1.0 } else { 0.0 },
        ),
    }
}

/// The slope at the outline of the dome the glint is computed over: the
/// strongest light's highlight then lands inside every front face, at about
/// a third of the way from the centre toward the light (brief 4b-5).
pub const GLINT_DOME: f32 = 1.0;

/// The face-shading parameters for `settings` ([`ToonExtension::face`]);
/// `w` (field present) is the material's own, set by
/// [`attach_face_fields`].
pub fn face_of(settings: &RenderSettings) -> Vec4 {
    Vec4::new(
        settings.relief.clamp(0.0, 1.0),
        GLINT_DOME,
        settings.bevel.clamp(0.01, 1.0),
        0.0,
    )
}

/// Installs the toon material pipeline and its embedded shader.
pub fn install(app: &mut App) {
    embedded_asset!(app, "toon.wesl");
    app.add_plugins(MaterialPlugin::<ToonMaterial>::default());
}

/// Pushes changed [`RenderSettings`] into every live body material, and
/// the ramp into every infinite plane (back plane, grounds), so the
/// shadows they catch take the same tones.
pub fn apply_render_settings(
    settings: Res<RenderSettings>,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut planes: ResMut<Assets<crate::plane::InfinitePlaneMaterial>>,
) {
    let params = params_of(&settings);
    let face = face_of(&settings);
    let ramp = ramp_of(&settings);
    let handles: Vec<_> = materials.ids().collect();
    for id in handles {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.params = params;
            let has_field = material.extension.face.w;
            material.extension.face = face.with_w(has_field);
            material.extension.ramp = ramp.clone();
        }
    }
    let handles: Vec<_> = planes.ids().collect();
    for id in handles {
        if let Some(mut material) = planes.get_mut(id) {
            material.extension.toon = params;
            material.extension.ramp = ramp.clone();
        }
    }
}

/// A body's baked face field ([`FaceField`](gradiance_geometry::face_field::FaceField)
/// as an image), rebuilt with its mesh (`extrude_sync`). Derived state.
#[derive(Component, Debug, Clone)]
pub struct FaceFieldImage(pub Handle<Image>);

/// Hands each body's face field to its material whenever either is new
/// (a shape edit bakes a field; an appearance edit builds a material).
pub fn attach_face_fields(
    mut materials: ResMut<Assets<ToonMaterial>>,
    bodies: Query<
        (&FaceFieldImage, &MeshMaterial3d<ToonMaterial>),
        Or<(
            Changed<FaceFieldImage>,
            Changed<MeshMaterial3d<ToonMaterial>>,
        )>,
    >,
) {
    for (field, material) in &bodies {
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.extension.field = Some(field.0.clone());
            material.extension.face.w = 1.0;
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // asserting exact sentinel values, not math
mod tests {
    use super::*;

    #[test]
    fn defaults_are_toon_with_a_ramp_and_a_rim() {
        let settings = RenderSettings::default();
        let params = params_of(&settings);
        assert_eq!(params.x, 1.0);
        assert!(params.y > 0.0, "rim on by default");
        assert_eq!(params.w, 0.0, "matte: no glint by default");
        let ramp = ramp_of(&settings);
        assert_eq!(ramp.params.w, 1.0);
        let first = settings.ramp.table()[0];
        assert_eq!(ramp.tones[0], Vec4::from_array(first));
    }

    #[test]
    fn disabling_toon_turns_the_ramp_off() {
        let settings = RenderSettings {
            toon_enabled: false,
            ..RenderSettings::default()
        };
        assert_eq!(params_of(&settings).x, 0.0);
        assert_eq!(ramp_of(&settings).params.w, 0.0);
    }

    #[test]
    fn dither_settings_are_clamped_to_what_the_shader_can_use() {
        let ramp = ramp_of(&RenderSettings {
            dither: f32::NAN,
            dither_scale: 0.0,
            ..RenderSettings::default()
        });
        assert_eq!(ramp.params.y, 0.0, "not finite = hard edges");
        assert_eq!(ramp.params.z, 1.0, "at least one pixel per cell");
        let linear = RenderSettings {
            ramp: gradiance_domain::ramp::ColorRamp {
                interp: RampInterp::Linear,
                ..RenderSettings::default().ramp
            },
            ..RenderSettings::default()
        };
        assert_eq!(ramp_of(&linear).params.x, 1.0);
    }

    #[test]
    fn studio_lights_its_faces_with_relief_and_flat_looks_do_not() {
        use gradiance_domain::theme::Look;
        let studio = face_of(&Look::Studio.render());
        assert!(studio.x > 0.0, "studio faces show relief");
        assert_eq!(studio.y, GLINT_DOME);
        assert!(studio.z > 0.0 && studio.z <= 1.0);
        for look in [Look::Blueprint, Look::Poster] {
            assert_eq!(face_of(&look.render()).x, 0.0, "{look:?} stays flat");
        }
        let negative = face_of(&RenderSettings {
            relief: -1.0,
            ..RenderSettings::default()
        });
        assert_eq!(negative.x, 0.0, "a negative relief is flat, not inverted");
    }

    /// The `ShaderRef` string and the `embedded_asset!` registration must
    /// agree; the registration path is derived from `module_path!()` (crate
    /// name with underscores) + the file's location under `src/`, so a crate
    /// rename or file move silently breaks the reference at runtime unless
    /// this pins them together. Must live in this file (same `file!()` as
    /// the registration in [`super::install`]).
    #[test]
    fn shader_ref_matches_the_embedded_registration() {
        let registered = bevy::asset::embedded_path!("toon.wesl");
        assert_eq!(
            format!("embedded://{}", registered.display()),
            TOON_SHADER_PATH
        );
    }
}
