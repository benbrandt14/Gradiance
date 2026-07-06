//! Always-on body decorations (Algodoo styling cues).

use crate::core::constants::LAYER_HEIGHT;
use crate::domain::Body;
use crate::domain::layers::LayerMask32;
use crate::domain::shape::ShapeDef;
use bevy::prelude::*;

/// Draws each circle's center-to-edge radius line (the classic Algodoo
/// rotation indicator — without it a spinning circle looks static).
pub fn draw_circle_radius_lines(
    bodies: Query<(&ShapeDef, &LayerMask32, &Transform), With<Body>>,
    mut gizmos: Gizmos,
) {
    for (shape, layers, transform) in &bodies {
        let ShapeDef::Circle { radius } = shape else {
            continue;
        };
        let center = transform.translation.truncate();
        let dir = Vec2::from_angle(transform.rotation.to_euler(EulerRot::ZYX).0);
        // Just in front of this body's front cap.
        let z = -(layers.occupied_range().map_or(0, |(min, _)| min) as f32) * LAYER_HEIGHT + 0.5;
        let tint = Color::srgba(0.05, 0.05, 0.05, 0.55);
        gizmos.line(center.extend(z), (center + dir * *radius).extend(z), tint);
    }
}
