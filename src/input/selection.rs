//! Entity selection management.
//!
//! Handles the currently selected entity and renders a highlight gizmo around it.

use crate::input::editable_shape::{EditableShape, ShapeType};
use crate::physics::floor::GroundPlane;
use bevy::prelude::*;
use std::collections::HashSet;

/// Filter for selection (which types of entities can be selected).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectionFilter {
    /// Select all types of entities.
    #[default]
    All,
    /// Select only shapes (RigidBody/Collider/Geometry).
    Shapes,
    /// Select only joints (Connectors).
    Joints,
}

/// Component for logical grouping of entities (Select one -> Select all).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct SelectionGroup(pub u32);

/// Resource for generating unique group IDs.
#[derive(Resource, Default)]
pub struct NextGroupID(pub u32);

/// Resource storing the currently selected entities.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selection(pub HashSet<Entity>);

impl Selection {
    /// Clears selection.
    pub fn clear(&mut self) {
        if !self.0.is_empty() {
            info!("Selection: Cleared");
            self.0.clear();
        }
    }
    /// Adds an entity.
    pub fn add(&mut self, entity: Entity) {
        if self.0.insert(entity) {
            info!("Selection: Added entity {:?}", entity);
        }
    }
    /// Removes an entity.
    pub fn remove(&mut self, entity: Entity) {
        if self.0.remove(&entity) {
            info!("Selection: Removed entity {:?}", entity);
        }
    }
    /// Toggles an entity.
    pub fn toggle(&mut self, entity: Entity) {
        if self.0.contains(&entity) {
            self.0.remove(&entity);
            info!("Selection: Removed entity {:?}", entity);
        } else {
            self.0.insert(entity);
            info!("Selection: Added entity {:?}", entity);
        }
    }
}

/// Plugin for selection visualization.
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Selection>();
        app.init_resource::<SelectionFilter>();
        app.init_resource::<NextGroupID>();
        app.add_systems(
            Update,
            (
                draw_selection_highlight,
                handle_delete_key,
                handle_grouping_input,
            ),
        );
    }
}

fn handle_grouping_input(
    mut commands: Commands,
    selection: Res<Selection>,
    keys: Res<ButtonInput<KeyCode>>,
    mut next_group_id: ResMut<NextGroupID>,
) {
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    if ctrl && keys.just_pressed(KeyCode::KeyG) {
        if shift {
            // Ungroup
            let mut count = 0;
            for &entity in &selection.0 {
                commands.entity(entity).remove::<SelectionGroup>();
                count += 1;
            }
            if count > 0 {
                info!("Ungrouped {} entities", count);
            }
        } else {
            // Group
            let id = next_group_id.0;
            next_group_id.0 += 1;
            let mut count = 0;
            for &entity in &selection.0 {
                commands.entity(entity).insert(SelectionGroup(id));
                count += 1;
            }
            if count > 0 {
                info!("Grouped {} entities into Group {}", count, id);
            }
        }
    }
}

fn handle_delete_key(
    mut commands: Commands,
    mut selection: ResMut<Selection>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        let count = selection.0.len();
        if count > 0 {
            info!("Deleted {} entities", count);
            for entity in selection.0.drain() {
                commands.entity(entity).despawn_recursive();
            }
        }
    }
}

/// System that draws a yellow outline around the selected entities.
///
/// Supports `EditableShape` and `GroundPlane`.
fn draw_selection_highlight(
    selection: Res<Selection>,
    query: Query<(&Transform, Option<&EditableShape>, Option<&GroundPlane>)>,
    mut gizmos: Gizmos,
) {
    for &entity in &selection.0 {
        if let Ok((transform, editable_shape, ground)) = query.get(entity) {
            let color = Color::srgb(1.0, 1.0, 0.0); // Yellow
            let t = transform.translation.truncate();
            let r = transform.rotation.to_euler(EulerRot::XYZ).2;

            let iso = Isometry2d::from_translation(t) * Isometry2d::from_rotation(Rot2::radians(r));

            if let Some(shape) = editable_shape {
                match &shape.shape {
                    ShapeType::Box { width, height } => {
                        gizmos.rect_2d(
                            iso,
                            Vec2::new(width + 0.2, height + 0.2), // slightly larger
                            color,
                        );
                    }
                    ShapeType::Circle { radius } => {
                        gizmos.circle_2d(iso, radius + 0.1, color);
                    }
                    ShapeType::Polygon { points } => {
                        if points.len() >= 3 {
                            for i in 0..points.len() {
                                let p1 = points[i];
                                let p2 = points[(i + 1) % points.len()];
                                let start = iso * p1;
                                let end = iso * p2;
                                gizmos.line_2d(start, end, color);
                            }
                        }
                    }
                }
            } else if ground.is_some() {
                // Visualize infinite plane selection (line + normal indicator)
                // Draw a very long line
                gizmos.line_2d(
                    iso * Vec2::new(-100_000.0, 0.0),
                    iso * Vec2::new(100_000.0, 0.0),
                    color,
                );
                // Draw normal indicators
                for x in (-10..=10).map(|i| i as f32 * 50.0) {
                    gizmos.line_2d(iso * Vec2::new(x, 0.0), iso * Vec2::new(x, 10.0), color);
                }
            } else {
                // Fallback
                gizmos.circle_2d(iso, 0.5, color);
            }
        }
    }
}
