//! Always-on body decorations (Algodoo styling cues).

use bevy::prelude::*;
use gradiance_domain::Body;
use gradiance_domain::appearance::Appearance;
use gradiance_domain::depth::DepthBand;
use gradiance_domain::shape::ShapeDef;
use gradiance_geometry::polygonize::polygonize;
use gradiance_interaction::overlay::OverlayGizmos;

/// Depth just in front of a body's front cap.
fn front_z(band: DepthBand) -> f32 {
    band.z_front() + 0.5
}

/// Draws each body's authored border outline (alpha 0 hides it).
pub fn draw_body_borders(
    bodies: Query<(&ShapeDef, &Appearance, &DepthBand, &Transform), With<Body>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    for (shape, appearance, layers, transform) in &bodies {
        let border = appearance.border;
        if border.a <= 0.01 || shape.contains_half_plane() {
            continue;
        }
        let color = Color::srgba(border.r, border.g, border.b, border.a);
        let z = front_z(*layers);
        let affine = transform.compute_affine();
        for ring in polygonize(shape).rings() {
            let mut points: Vec<Vec3> = ring
                .iter()
                .map(|v| {
                    let w = affine.transform_point3(v.extend(0.0));
                    Vec3::new(w.x, w.y, z)
                })
                .collect();
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            gizmos.linestrip(points, color);
        }
    }
}

/// Draws each circle's center-to-edge radius line (the classic Algodoo
/// rotation indicator — without it a spinning circle looks static).
pub fn draw_circle_radius_lines(
    bodies: Query<(&ShapeDef, &DepthBand, &Transform), With<Body>>,
    mut gizmos: Gizmos<OverlayGizmos>,
) {
    for (shape, layers, transform) in &bodies {
        let ShapeDef::Circle { radius } = shape else {
            continue;
        };
        let center = transform.translation.truncate();
        let dir = Vec2::from_angle(transform.rotation.to_euler(EulerRot::ZYX).0);
        let z = front_z(*layers);
        let tint = Color::srgba(0.05, 0.05, 0.05, 0.55);
        gizmos.line(center.extend(z), (center + dir * *radius).extend(z), tint);
    }
}
