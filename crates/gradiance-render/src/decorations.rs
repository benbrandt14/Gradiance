//! Always-on body decorations (Algodoo styling cues).

use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::fluid::Fluid;
use gradiance_domain::settings::{RenderSettings, SimSettings};
use gradiance_domain::shape::ShapeDef;
use gradiance_domain::theme::active;
use gradiance_geometry::contours::Contours;
use gradiance_geometry::polygonize::ShapeContours;
use gradiance_interaction::overlay::OverlayGizmos;

/// Depth just in front of a body's front cap: a 5 mm world lift, enough to
/// clear the fill face's depth without the outline visibly floating in
/// front (the world is SI metres — a larger lift parallaxes off the body
/// when the view tilts).
/// `transform` carries the body's stacking offset within its band
/// (`stack`), so an older body's outline sinks with its face and never
/// shows through a newer body on top of it.
fn front_z(band: DepthBand, transform: &Transform) -> f32 {
    band.z_front() + transform.translation.z + 0.005
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
        let z = front_z(*layers, transform);
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
        let z = front_z(*layers, transform);
        gizmos.line(center.extend(z), (center + dir * *radius).extend(z), tint);
    }
}

/// How far toward white a fluid's surface line is from its fill (brief Q-5).
const SURFACE_LIGHTEN: f32 = 0.75;

/// Half the length a sea's surface line is drawn (m): past any view.
const SEA_HALF_SPAN: f32 = 5_000.0;

/// The surface line's colour: the fluid's displayed fill, lightened, opaque.
#[must_use]
pub fn surface_colour(
    fill: gradiance_domain::appearance::Rgba,
) -> gradiance_domain::appearance::Rgba {
    let lifted = fill.lerp(gradiance_domain::theme::FLUID_SURFACE_LIFT, SURFACE_LIGHTEN);
    gradiance_domain::appearance::Rgba { a: 1.0, ..lifted }
}

/// Draws each fluid's **water surface** (brief Q-5): the outline edges that
/// face up (against gravity, within 60 degrees,
/// `geometry::submerge::surface_edges`) as one line in a lighter tint of
/// its fill, in front of the fill. A sea (a half-plane fluid) draws its
/// surface across the view. Reads the cached [`ShapeContours`]; `O(V)` per
/// fluid for `V` outline vertices, no allocation, no contouring.
pub fn draw_fluid_surfaces(
    fluids: Query<
        (
            &ShapeDef,
            &ShapeContours,
            &Appearance,
            &DepthBand,
            &Transform,
        ),
        (With<Body>, With<Fluid>),
    >,
    settings: Option<Res<RenderSettings>>,
    sim: Option<Res<SimSettings>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    if fluids.is_empty() {
        return;
    }
    let theme = active(settings.as_deref());
    let up = sim
        .map(|s| -s.gravity.0)
        .and_then(Vec2::try_normalize)
        .unwrap_or(Vec2::Y);
    for (shape, contours, appearance, band, transform) in &fluids {
        let color = Color::from(surface_colour(theme.display(appearance.fill)));
        let z = front_z(*band, transform);
        let affine = transform.compute_affine();
        let to_world = |v: Vec2| {
            let w = affine.transform_point3(v.extend(0.0));
            Vec3::new(w.x, w.y, z)
        };
        if matches!(shape, ShapeDef::HalfPlane) {
            gizmos.line(
                to_world(Vec2::new(-SEA_HALF_SPAN, 0.0)),
                to_world(Vec2::new(SEA_HALF_SPAN, 0.0)),
                color,
            );
            continue;
        }
        let local_up = (transform.rotation.inverse() * up.extend(0.0)).truncate();
        for component in contours.iter() {
            for (p, q) in gradiance_geometry::submerge::surface_edges(component, local_up) {
                gizmos.line(to_world(p), to_world(q), color);
            }
        }
    }
}
