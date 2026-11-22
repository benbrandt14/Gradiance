//! Authored appearance → derived body material (toon-banded matte).
//!
//! A [`SignalColorOverride`] (the signal dataflow's derived tint) is
//! preferred over the authored fill while present; removing it restores
//! the authored appearance. The authored fill shows in the active look's
//! tone ([`SceneTheme::display`]), so a look change rebuilds every body's
//! material.

use crate::toon::{ToonExtension, ToonMaterial, face_of, params_of, ramp_of};
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::fluid::Fluid;
use gradiance_domain::optics::Optics;
use gradiance_domain::settings::RenderSettings;
use gradiance_domain::theme::{Look, SceneTheme};
use gradiance_signal::SignalColorOverride;

/// Converts a domain color into a Bevy color (sRGB).
pub fn color_of(rgba: Rgba) -> Color {
    rgba.into()
}

/// How opaque a fluid region draws in the default look (brief N-7):
/// translucent, so what floats or sinks in it stays visible. Each
/// [`FluidLook`](gradiance_domain::settings::FluidLook) has its own.
pub const FLUID_OPACITY: f32 = 0.45;

/// How opaque a transparent body (one with a refractive index, brief S-3)
/// draws: shaded like a body, but see-through, so the beam inside shows.
pub const GLASS_OPACITY: f32 = 0.55;

/// The standard body material: matte base, toon banding per settings. A
/// fluid region blends at [`FLUID_OPACITY`].
fn body_material(
    appearance: &Appearance,
    tint: Option<Color>,
    translucency: Option<f32>,
    settings: &RenderSettings,
    theme: &SceneTheme,
) -> ToonMaterial {
    let fill = match tint {
        Some(color) => {
            let c = color.to_srgba();
            Rgba {
                r: c.red,
                g: c.green,
                b: c.blue,
                a: c.alpha,
            }
        }
        None => theme.display(appearance.fill),
    };
    let (fill, alpha_mode) = match translucency {
        Some(opacity) => (
            Rgba {
                a: fill.a * opacity,
                ..fill
            },
            AlphaMode::Blend,
        ),
        None => (fill, AlphaMode::Opaque),
    };
    ToonMaterial {
        base: StandardMaterial {
            base_color: color_of(fill),
            // Fully matte: with any PBR specular (roughness < 1 or
            // nonzero reflectance) the banded luminance is view-dependent
            // and every front face — all sharing one normal — jumps bands
            // *simultaneously* as the camera tilts, snapping the whole
            // scene's color. A lit patch is a ramp tone instead (brief
            // 4b-6), and the optional glint is the shader's.
            alpha_mode,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            metallic: 0.0,
            // Double-sided: kept as a guard. The extruded caps are wound to
            // face their normals (`geometry::extrusion`), but a cap that
            // ever wound away would vanish under back-face culling.
            double_sided: true,
            cull_mode: None,
            // Emissive glow in the fill's hue (0 = matte). Alpha 0 opts the
            // glow out of camera exposure: with alpha 1 Bevy multiplies it
            // by the exposure (about 1/1000 at the default EV), and a body
            // at emissive 2 glowed by 0.2 % — invisible (brief 4b-3).
            emissive: {
                let fill = LinearRgba::from(Color::from(fill));
                LinearRgba {
                    alpha: 0.0,
                    ..fill * appearance.emissive
                }
            },
            ..default()
        },
        extension: ToonExtension {
            params: params_of(settings),
            face: face_of(settings),
            ramp: ramp_of(settings),
            // `toon::attach_face_fields` hands over the body's baked field.
            field: None,
        },
    }
}

/// A fluid's material in an unshaded [`FluidLook`]: an even, unlit tint of
/// the fill (darkened toward [`FLUID_DEPTH_TONE`] by the look's depth
/// shade) at the look's opacity. No toon bands, no relief: water is not a
/// solid body.
///
/// [`FluidLook`]: gradiance_domain::settings::FluidLook
/// [`FLUID_DEPTH_TONE`]: gradiance_domain::theme::FLUID_DEPTH_TONE
fn water_material(fill: Rgba, settings: &RenderSettings) -> StandardMaterial {
    let look = settings.fluid_look;
    let tinted = fill.lerp(
        gradiance_domain::theme::FLUID_DEPTH_TONE,
        look.depth_shade(),
    );
    StandardMaterial {
        base_color: color_of(Rgba {
            a: fill.a * look.opacity(),
            ..tinted
        }),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}

/// (Re)builds a body's material when its appearance, signal tint or fluid
/// flag changes (or the tint or the fluid is removed — the authored fill
/// returns), and every fluid's when the fluid look changes.
#[expect(
    clippy::too_many_arguments,
    reason = "the sync's queries and both material stores"
)]
pub fn sync_body_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<ToonMaterial>>,
    mut water: ResMut<Assets<StandardMaterial>>,
    mut shown_fluid_look: Local<Option<gradiance_domain::settings::FluidLook>>,
    settings: Res<RenderSettings>,
    changed: Query<
        (
            Entity,
            &Appearance,
            Option<&SignalColorOverride>,
            &gradiance_domain::shape::ShapeDef,
            Has<Fluid>,
            Has<Optics>,
        ),
        (
            With<Body>,
            Or<(
                Changed<Appearance>,
                Changed<SignalColorOverride>,
                Changed<Fluid>,
                Changed<Optics>,
            )>,
        ),
    >,
    bodies: Query<
        (
            Entity,
            &Appearance,
            Option<&SignalColorOverride>,
            &gradiance_domain::shape::ShapeDef,
            Has<Fluid>,
            Has<Optics>,
        ),
        With<Body>,
    >,
    mut untinted: RemovedComponents<SignalColorOverride>,
    mut drained: RemovedComponents<Fluid>,
    mut cleared: RemovedComponents<Optics>,
    mut shown_look: Local<Option<Look>>,
) {
    let removed: Vec<Entity> = untinted
        .read()
        .chain(drained.read())
        .chain(cleared.read())
        .collect();
    // A new look re-tones every body: rebuild them all, once.
    let relook = *shown_look != Some(settings.look);
    *shown_look = Some(settings.look);
    let theme = settings.look.theme();
    // A new fluid look rebuilds the fluids (and only them).
    let refluid = *shown_fluid_look != Some(settings.fluid_look);
    *shown_fluid_look = Some(settings.fluid_look);
    let all: Vec<Entity> = if relook {
        bodies.iter().map(|(e, ..)| e).collect()
    } else if refluid {
        bodies
            .iter()
            .filter(|(.., fluid, _)| *fluid)
            .map(|(e, ..)| e)
            .collect()
    } else {
        Vec::new()
    };
    for (entity, appearance, tint, shape, fluid, glass) in changed.iter().chain(
        removed
            .iter()
            .chain(&all)
            .filter_map(|e| bodies.get(*e).ok()),
    ) {
        // Half-plane grounds render through `render::ground`'s material.
        if matches!(shape, gradiance_domain::shape::ShapeDef::HalfPlane) {
            continue;
        }
        if fluid && !settings.fluid_look.shaded() {
            let fill = tint.and_then(|t| t.fill).map_or_else(
                || theme.display(appearance.fill),
                |c| {
                    let c = c.to_srgba();
                    Rgba {
                        r: c.red,
                        g: c.green,
                        b: c.blue,
                        a: c.alpha,
                    }
                },
            );
            let handle = water.add(water_material(fill, &settings));
            commands
                .entity(entity)
                .remove::<MeshMaterial3d<ToonMaterial>>()
                .insert(MeshMaterial3d(handle));
            continue;
        }
        let translucency = if fluid {
            Some(settings.fluid_look.opacity())
        } else if glass {
            Some(GLASS_OPACITY)
        } else {
            None
        };
        let handle = materials.add(body_material(
            appearance,
            tint.and_then(|t| t.fill),
            translucency,
            &settings,
            theme,
        ));
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert(MeshMaterial3d(handle));
    }
}
