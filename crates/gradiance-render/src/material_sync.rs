//! Authored appearance → derived body material (toon-banded matte).
//!
//! A [`SignalColorOverride`] (the signal dataflow's derived tint) is
//! preferred over the authored fill while present; removing it restores
//! the authored appearance. The authored fill shows in the active look's
//! tone ([`SceneTheme::display`]), so a look change rebuilds every body's
//! material.

use crate::toon::{ToonExtension, ToonMaterial, params_of};
use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::{Appearance, Rgba};
use gradiance_domain::settings::RenderSettings;
use gradiance_domain::theme::{Look, SceneTheme};
use gradiance_signal::SignalColorOverride;

/// Converts a domain color into a Bevy color (sRGB).
pub fn color_of(rgba: Rgba) -> Color {
    rgba.into()
}

/// The standard body material: matte base, toon banding per settings.
fn body_material(
    appearance: &Appearance,
    tint: Option<Color>,
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
        },
    }
}

/// (Re)builds a body's material when its appearance or signal tint
/// changes (or the tint is removed — the authored fill returns).
pub fn sync_body_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<ToonMaterial>>,
    settings: Res<RenderSettings>,
    changed: Query<
        (
            Entity,
            &Appearance,
            Option<&SignalColorOverride>,
            &gradiance_domain::shape::ShapeDef,
        ),
        (
            With<Body>,
            Or<(Changed<Appearance>, Changed<SignalColorOverride>)>,
        ),
    >,
    bodies: Query<
        (
            Entity,
            &Appearance,
            Option<&SignalColorOverride>,
            &gradiance_domain::shape::ShapeDef,
        ),
        With<Body>,
    >,
    mut untinted: RemovedComponents<SignalColorOverride>,
    mut shown_look: Local<Option<Look>>,
) {
    let removed: Vec<Entity> = untinted.read().collect();
    // A new look re-tones every body: rebuild them all, once.
    let relook = *shown_look != Some(settings.look);
    *shown_look = Some(settings.look);
    let theme = settings.look.theme();
    let all: Vec<Entity> = if relook {
        bodies.iter().map(|(e, ..)| e).collect()
    } else {
        Vec::new()
    };
    for (entity, appearance, tint, shape) in changed.iter().chain(
        removed
            .iter()
            .chain(&all)
            .filter_map(|e| bodies.get(*e).ok()),
    ) {
        // Half-plane grounds render through `render::ground`'s material.
        if matches!(shape, gradiance_domain::shape::ShapeDef::HalfPlane) {
            continue;
        }
        let handle = materials.add(body_material(
            appearance,
            tint.and_then(|t| t.fill),
            &settings,
            theme,
        ));
        commands.entity(entity).insert(MeshMaterial3d(handle));
    }
}
