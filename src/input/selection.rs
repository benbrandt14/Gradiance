//! Entity selection management.
//!
//! Handles the currently selected entity and renders a highlight gizmo around it.

use crate::input::editable::{EditableBox, EditableCircle};
use bevy::prelude::*;
// use avian2d::prelude::*; // Not needed directly here if using standard math

/// Resource storing the currently selected entity, if any.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection(pub Option<Entity>);

/// Plugin for selection visualization.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>();
        app.add_systems(Update, draw_selection_highlight);
    }
}

/// System that draws a yellow outline around the selected entity.
///
/// Supports `EditableBox` and `EditableCircle` shapes.
fn draw_selection_highlight(
    selection: Res<Selection>,
    query: Query<(&Transform, Option<&EditableBox>, Option<&EditableCircle>)>,
    mut gizmos: Gizmos,
) {
    if let Some(entity) = selection.0 {
        if let Ok((transform, box_shape, circle_shape)) = query.get(entity) {
            let color = Color::srgb(1.0, 1.0, 0.0); // Yellow
            let t = transform.translation.truncate();
            let r = transform.rotation.to_euler(EulerRot::XYZ).2;

            let iso = Isometry2d::from_translation(t) * Isometry2d::from_rotation(Rot2::radians(r));

            if let Some(b) = box_shape {
                gizmos.rect_2d(
                    iso,
                    Vec2::new(b.width as f32 + 0.2, b.height as f32 + 0.2), // slightly larger
                    color,
                );
            } else if let Some(c) = circle_shape {
                gizmos.circle_2d(iso, c.radius as f32 + 0.1, color);
            } else {
                // Fallback
                gizmos.circle_2d(iso, 0.5, color);
            }
        }
    }
}
