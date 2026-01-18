//! Entity selection management.
//!
//! Handles the currently selected entity and renders a highlight gizmo around it.

use crate::input::editable::{EditableBox, EditableCircle};
use crate::GroundPlane;
use bevy::prelude::*;
use std::collections::HashSet;

/// Resource storing the currently selected entities.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selection(pub HashSet<Entity>);

impl Selection {
    /// Clears selection.
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Adds an entity.
    pub fn add(&mut self, entity: Entity) {
        self.0.insert(entity);
    }
    /// Removes an entity.
    pub fn remove(&mut self, entity: Entity) {
        self.0.remove(&entity);
    }
    /// Toggles an entity.
    pub fn toggle(&mut self, entity: Entity) {
        if self.0.contains(&entity) {
            self.0.remove(&entity);
        } else {
            self.0.insert(entity);
        }
    }
}

/// Plugin for selection visualization.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>();
        app.add_systems(Update, (draw_selection_highlight, handle_delete_key));
    }
}

fn handle_delete_key(
    mut commands: Commands,
    mut selection: ResMut<Selection>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        for entity in selection.0.drain() {
             commands.entity(entity).despawn();
        }
    }
}

/// System that draws a yellow outline around the selected entities.
///
/// Supports `EditableBox`, `EditableCircle`, and `GroundPlane`.
fn draw_selection_highlight(
    selection: Res<Selection>,
    query: Query<(
        &Transform,
        Option<&EditableBox>,
        Option<&EditableCircle>,
        Option<&GroundPlane>,
    )>,
    mut gizmos: Gizmos,
) {
    for &entity in &selection.0 {
        if let Ok((transform, box_shape, circle_shape, ground)) = query.get(entity) {
            let color = Color::srgb(1.0, 1.0, 0.0); // Yellow
            let t = transform.translation.truncate();
            let r = transform.rotation.to_euler(EulerRot::XYZ).2;

            if let Some(b) = box_shape {
                gizmos.rect_2d(
                    t,
                    r,
                    Vec2::new(b.width as f32 + 0.2, b.height as f32 + 0.2), // slightly larger
                    color,
                );
            } else if let Some(c) = circle_shape {
                gizmos.circle_2d(t, c.radius as f32 + 0.1, color);
            } else if ground.is_some() {
                // Visualize infinite plane selection (line + normal indicator)
                // Draw a very long line
                let dir_x = Vec2::new(r.cos(), r.sin());
                let dir_y = Vec2::new(-r.sin(), r.cos());

                gizmos.line_2d(
                    t - dir_x * 100_000.0,
                    t + dir_x * 100_000.0,
                    color,
                );
                // Draw normal indicators
                for x in (-10..=10).map(|i| i as f32 * 50.0) {
                     gizmos.line_2d(
                        t + dir_x * x,
                        t + dir_x * x + dir_y * 10.0,
                        color,
                    );
                }
            } else {
                // Fallback
                gizmos.circle_2d(t, 0.5, color);
            }
        }
    }
}
