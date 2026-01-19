//! Shared utilities for tools.

use bevy::prelude::*;
use bevy_egui::EguiContexts;

/// Calculates the local position of a point relative to a body's transform.
///
/// This transforms a point in world space into the local space of the given transform,
/// accounting for both translation and rotation (assuming 2D rotation around Z).
pub fn calculate_local_anchor(transform: &Transform, world_point: Vec2) -> Vec2 {
    let rotation = transform.rotation.to_euler(EulerRot::XYZ).2;
    let translation = transform.translation.truncate();
    let relative = world_point - translation;

    let cos = rotation.cos();
    let sin = rotation.sin();

    // Rotate relative vector by -rotation (inverse rotation)
    // R^-1 = R^T
    // [ cos  sin ] [ x ]   [ x cos + y sin ]
    // [ -sin cos ] [ y ]   [ -x sin + y cos ]
    Vec2::new(
        relative.x * cos + relative.y * sin,
        -relative.x * sin + relative.y * cos,
    )
}

/// Checks if the mouse pointer is currently over an Egui area (window/panel).
///
/// Returns true if the pointer is over an Egui area, false otherwise.
/// Returns false if Egui context is not available (e.g. headless tests).
pub fn is_pointer_over_ui(contexts: &mut EguiContexts) -> bool {
    if let Some(ctx) = contexts.try_ctx_mut() {
        ctx.is_pointer_over_area()
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        Transform::from_translation(Vec3::new(10.0, 10.0, 0.0)),
        Vec2::new(15.0, 15.0),
        Vec2::new(5.0, 5.0)
    )]
    #[case(
        // Rotated 90 degrees around Z (counter-clockwise)
        Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
        Vec2::new(1.0, 0.0),
        Vec2::new(0.0, -1.0)
    )]
    fn test_calculate_local_anchor(
        #[case] transform: Transform,
        #[case] world_point: Vec2,
        #[case] expected: Vec2,
    ) {
        let result = calculate_local_anchor(&transform, world_point);
        assert!((result.x - expected.x).abs() < 1e-5);
        assert!((result.y - expected.y).abs() < 1e-5);
    }
}
