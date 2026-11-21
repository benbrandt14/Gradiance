//! Always-on body decorations (Algodoo styling cues).

use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::settings::RenderSettings;
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::theme::active;
use gradiance_geometry::contours::Contours;
use gradiance_geometry::polygonize::ShapeContours;
use gradiance_interaction::overlay::OverlayGizmos;

/// Depth just in front of a body's front cap: a 5 mm world lift, enough to
/// clear the fill face's depth without the outline visibly floating in
/// front (the world is SI metres — a larger lift parallaxes off the body
/// when the view tilts).
fn front_z(band: DepthBand) -> f32 {
    band.z_front() + 0.005
}

/// Draws each body's authored border outline (alpha 0 hides it) from the
/// cached [`ShapeContours`]: every connected component, matching the
/// extruded mesh — re-contouring the SDF per frame was the border pass's
/// cost at high body counts.
pub fn draw_body_borders(
    bodies: Query<
        (
            &ShapeDef,
            &ShapeContours,
            &Appearance,
            &DepthBand,
            &Transform,
        ),
        With<Body>,
    >,
    settings: Option<Res<RenderSettings>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let theme = active(settings.as_deref());
    for (shape, contours, appearance, layers, transform) in &bodies {
        let border = theme.display(appearance.border);
        if border.a <= 0.01 || shape.contains_half_plane() {
            continue;
        }
        let color = Color::from(border);
        let z = front_z(*layers);
        let affine = transform.compute_affine();
        for ring in contours.iter().flat_map(Contours::rings) {
            // Close the loop without collecting: `linestrip` takes an
            // iterator.
            let to_world = |v: &bevy::math::Vec2| {
                let w = affine.transform_point3(v.extend(0.0));
                Vec3::new(w.x, w.y, z)
            };
            gizmos.linestrip(
                ring.iter().map(to_world).chain(ring.first().map(to_world)),
                color,
            );
        }
    }
}

/// Draws each circle's center-to-edge radius line (the classic Algodoo
/// rotation indicator — without it a spinning circle looks static).
pub fn draw_circle_radius_lines(
    bodies: Query<(&ShapeDef, &DepthBand, &Transform), With<Body>>,
    settings: Option<Res<RenderSettings>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    let tint = Color::from(active(settings.as_deref()).ink.muted.with_alpha(0.55));
    for (shape, layers, transform) in &bodies {
        let ShapeDef::Circle { radius } = shape else {
            continue;
        };
        let center = transform.translation.truncate();
        let dir = Vec2::from_angle(transform.rotation.to_euler(EulerRot::ZYX).0);
        let z = front_z(*layers);
        gizmos.line(center.extend(z), (center + dir * *radius).extend(z), tint);
    }
}
