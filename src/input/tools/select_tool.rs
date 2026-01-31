//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::events::{
    CommitDragEvent, DragEntitiesEvent, DuplicateEntitiesEvent, PropertyChange, PropertyChangeEvent,
};
use crate::input::selection::{Selection, SelectionFilter, SelectionGroup};
use crate::input::tools::connector::Connector;
use crate::input::{PointerOverUi, ToolState, cursor::CursorWorldPos};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use crate::ui::grid::GridSettings;
use bevy::ecs::system::SystemParam;
use bevy_rapier2d::prelude::*;

/// Auxiliary queries used by the select tool.
#[derive(SystemParam)]
pub struct AuxQueries<'w, 's> {
    /// Selectable entities.
    pub selectable:
        Query<'w, 's, (Entity, &'static GlobalTransform), (With<Collider>, Without<GroundPlane>)>,
    /// Connectors.
    pub connector: Query<'w, 's, &'static Connector>,
    /// Selection groups.
    pub group: Query<'w, 's, (Entity, &'static SelectionGroup)>,
    /// Transforms for drag/rotate calculations.
    pub transforms: Query<'w, 's, &'static Transform>,
}

/// Information needed to sort hits for selection.
#[derive(Debug, PartialEq)]
pub struct HitSortInfo {
    /// Whether the entity is a ground plane.
    pub is_ground: bool,
    /// The Z-index of the entity.
    pub z_index: f32,
}

/// Compare two hits for selection priority.
pub fn compare_hits(a: &HitSortInfo, b: &HitSortInfo) -> std::cmp::Ordering {
    let ground_order = a.is_ground.cmp(&b.is_ground);
    if ground_order != std::cmp::Ordering::Equal {
        return ground_order;
    }
    b.z_index
        .partial_cmp(&a.z_index)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Plugin for the Select Tool.
pub struct SelectToolPlugin;

impl Plugin for SelectToolPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectToolData>();
        app.add_systems(
            Update,
            select_tool_update
                .run_if(in_state(ToolState::Select))
                .before(crate::input::camera_controller::camera_pan),
        );
        app.add_systems(OnExit(ToolState::Select), select_tool_reset);
    }
}

/// Runtime data for the Select Tool.
#[derive(Resource, Default)]
pub struct SelectToolData {
    /// Start position of the drag box.
    pub drag_start: Option<Vec2>,
    /// Whether entities are being moved.
    pub is_moving: bool,
    /// Store initial state for move/rotate: Map Entity -> (Initial Pos, Initial Rot)
    pub initial_state: Vec<(Entity, Vec2, f32)>,
    /// Start position of the drag move (mouse pos).
    pub drag_start_pos: Vec2,

    /// Whether entities are being rotated.
    pub is_rotating: bool,
    /// Start position of the rotation drag (mouse pos).
    pub rotate_start_pos: Vec2,
    /// Centroid of the rotation.
    pub rotation_centroid: Vec2,
}

fn select_tool_reset(mut data: ResMut<SelectToolData>) {
    *data = SelectToolData::default();
}

#[allow(clippy::too_many_arguments)]
fn select_tool_update(
    mut selection: ResMut<Selection>,
    selection_filter: Res<SelectionFilter>,
    mut data: ResMut<SelectToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    rapier_context_query: Query<&RapierContext>,
    pointer_over_ui: Res<PointerOverUi>,
    mut gizmos: Gizmos,
    aux: AuxQueries,
    _grid_settings: Res<GridSettings>,
    // Events
    mut ev_duplicate: EventWriter<DuplicateEntitiesEvent>,
    mut ev_drag: EventWriter<DragEntitiesEvent>,
    mut ev_commit: EventWriter<CommitDragEvent>,
    mut ev_prop: EventWriter<PropertyChangeEvent>,
) {
    if pointer_over_ui.0 && !data.is_moving && !data.is_rotating && data.drag_start.is_none() {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    // Left Click Handling
    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start_pos = current_pos;

        // Hit Test
        let mut hit_entity = None;
        if let Some(rapier_context) = rapier_context_query.iter().next() {
            let filter = QueryFilter::default().exclude_sensors();
            let mut hits = Vec::new();
            rapier_context.intersections_with_point(current_pos, filter, |entity| {
                let is_connector = aux.connector.contains(entity);
                let pass = match *selection_filter {
                    SelectionFilter::All => true,
                    SelectionFilter::Shapes => !is_connector,
                    SelectionFilter::Joints => is_connector,
                };
                if pass {
                    hits.push(entity);
                }
                true
            });

            // Sort hits
            hits.sort_by(|&a, &b| {
                let get_info = |entity| -> Option<HitSortInfo> {
                    if let Ok((_, t)) = aux.selectable.get(entity) {
                        Some(HitSortInfo {
                            is_ground: false,
                            z_index: t.translation().z,
                        })
                    } else {
                        None
                    }
                };
                match (get_info(a), get_info(b)) {
                    (Some(ia), Some(ib)) => compare_hits(&ia, &ib),
                    _ => std::cmp::Ordering::Equal,
                }
            });

            hit_entity = hits.first().copied();
        }

        if let Some(entity) = hit_entity {
            // Group Logic
            let mut entities_to_select = Vec::new();
            if let Ok((_, group_id)) = aux.group.get(entity) {
                for (e, g) in &aux.group {
                    if g == group_id {
                        entities_to_select.push(e);
                    }
                }
            } else {
                entities_to_select.push(entity);
            }

            // Selection Logic
            let clicked_already_selected = selection.0.contains(&entity);
            if !shift && !clicked_already_selected {
                selection.clear();
            }

            if shift {
                let any_selected = entities_to_select.iter().any(|e| selection.0.contains(e));
                if any_selected {
                    for e in entities_to_select { selection.remove(e); }
                } else {
                    for e in entities_to_select { selection.add(e); }
                }
            } else if !clicked_already_selected {
                for e in entities_to_select { selection.add(e); }
            }

            // Duplicate or Move
            if ctrl && selection.0.contains(&entity) {
                // Duplicate
                let entities: Vec<Entity> = selection.0.iter().copied().collect();
                ev_duplicate.send(DuplicateEntitiesEvent {
                    entities,
                    make_kinematic: true,
                });

                data.is_moving = true;
                data.initial_state.clear();

            } else if selection.0.contains(&entity) {
                // Move
                data.is_moving = true;
                // Capture initial state
                data.initial_state.clear();
                for &e in &selection.0 {
                    if let Ok(t) = aux.transforms.get(e) {
                        data.initial_state.push((e, t.translation.truncate(), t.rotation.to_euler(EulerRot::XYZ).2));
                        // Set Kinematic
                        ev_prop.send(PropertyChangeEvent {
                            entity: e,
                            change: PropertyChange::RigidBody(RigidBody::KinematicPositionBased),
                        });
                    }
                }
            }
        } else {
            // Clicked empty space
            if !shift {
                selection.clear();
            }
            data.is_moving = false;
            data.drag_start = Some(current_pos);
        }
    }

    // Logic to re-populate initial_state if selection changed (e.g. duplication finished)
    if data.is_moving && data.initial_state.is_empty() && !selection.0.is_empty() {
        // Selection populated (likely from duplication)
        for &e in &selection.0 {
             if let Ok(t) = aux.transforms.get(e) {
                data.initial_state.push((e, t.translation.truncate(), t.rotation.to_euler(EulerRot::XYZ).2));
                // Ensure Kinematic (DuplicateEntitiesEvent handles it, but just in case)
                ev_prop.send(PropertyChangeEvent {
                    entity: e,
                    change: PropertyChange::RigidBody(RigidBody::KinematicPositionBased),
                });
            }
        }
        // Update drag_start_pos to current mouse pos to avoid jump
        data.drag_start_pos = current_pos;
    }

    // Drag Update
    if mouse.pressed(MouseButton::Left) {
        if data.is_moving {
            let delta = current_pos - data.drag_start_pos;
            let mut updates = Vec::new();

            for (entity, initial_pos, _) in &data.initial_state {
                let new_pos = *initial_pos + delta;
                updates.push((*entity, new_pos));
            }

            if !updates.is_empty() {
                ev_drag.send(DragEntitiesEvent {
                    positions: updates,
                    rotations: Vec::new(),
                });
            }
        } else if let Some(start) = data.drag_start {
            // Box Select Visual
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;
            let center = (min + max) / 2.0;
            gizmos.rect_2d(
                Isometry2d::from_translation(center),
                size,
                Color::srgb(0.0, 1.0, 1.0),
            );
        }
    }

    // Drag End
    if mouse.just_released(MouseButton::Left) {
        if data.is_moving {
            let delta = current_pos - data.drag_start_pos;

            // Commit
            let mut pos_changes = Vec::new();
            for (entity, initial_pos, _) in &data.initial_state {
                 let new_pos = *initial_pos + delta;
                 pos_changes.push((*entity, *initial_pos, new_pos));

                 // Restore Dynamic
                 ev_prop.send(PropertyChangeEvent {
                    entity: *entity,
                    change: PropertyChange::RigidBody(RigidBody::Dynamic),
                 });
                 // Also restore sleeping enabled?
                 ev_prop.send(PropertyChangeEvent {
                    entity: *entity,
                    change: PropertyChange::Restitution(0.0),
                 });
            }

            if !pos_changes.is_empty() {
                 ev_commit.send(CommitDragEvent {
                     position_changes: pos_changes,
                     rotation_changes: Vec::new(),
                 });
                 info!("Moved {} entities", data.initial_state.len());
            }
        } else if let Some(start) = data.drag_start {
            // Box Select Finalize
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;

            if size.x > 0.1 && size.y > 0.1 {
                let mut count = 0;
                 for (entity, global_transform) in &aux.selectable {
                    let t = global_transform.translation().truncate();
                    if is_point_in_box(t, min, max) {
                        if selection.0.insert(entity) {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    info!("Box Selected {} entities", count);
                }
            }
        }

        data.is_moving = false;
        data.drag_start = None;
        data.initial_state.clear();
    }

    // Right Click Rotation
    if mouse.just_pressed(MouseButton::Right) && !selection.0.is_empty() {
        let mut pointer_over_selection = false;
         if let Some(rapier_context) = rapier_context_query.iter().next() {
            let filter = QueryFilter::default().exclude_sensors();
             rapier_context.intersections_with_point(current_pos, filter, |entity| {
                if selection.0.contains(&entity) {
                    pointer_over_selection = true;
                    false
                } else {
                    true
                }
            });
        }

        if pointer_over_selection {
            data.is_rotating = true;
            data.rotate_start_pos = current_pos;
            data.initial_state.clear();

            let mut centroid = Vec2::ZERO;
            let mut count = 0.0;

            for &e in &selection.0 {
                if let Ok(t) = aux.transforms.get(e) {
                     let pos = t.translation.truncate();
                     let rot = t.rotation.to_euler(EulerRot::XYZ).2;
                     data.initial_state.push((e, pos, rot));
                     centroid += pos;
                     count += 1.0;

                     // Set Kinematic
                     ev_prop.send(PropertyChangeEvent {
                        entity: e,
                        change: PropertyChange::RigidBody(RigidBody::KinematicPositionBased),
                     });
                }
            }
            if count > 0.0 {
                data.rotation_centroid = centroid / count;
            }
        }
    }

    if mouse.pressed(MouseButton::Right) && data.is_rotating {
        let delta = current_pos - data.rotate_start_pos;
        let mut pos_updates = Vec::new();
        let mut rot_updates = Vec::new();

        for (entity, initial_pos, initial_rot) in &data.initial_state {
             let (new_pos, new_rot) = calculate_rotation_update(
                *initial_pos,
                *initial_rot,
                data.rotation_centroid,
                delta
            );
            pos_updates.push((*entity, new_pos));
            rot_updates.push((*entity, Quat::from_rotation_z(new_rot)));
        }

        ev_drag.send(DragEntitiesEvent {
            positions: pos_updates,
            rotations: rot_updates,
        });
    }

    if mouse.just_released(MouseButton::Right) {
        if data.is_rotating {
             // Commit
             let delta = current_pos - data.rotate_start_pos;
             let mut pos_changes = Vec::new();
             let mut rot_changes = Vec::new();

             for (entity, initial_pos, initial_rot) in &data.initial_state {
                 let (new_pos, new_rot) = calculate_rotation_update(
                    *initial_pos,
                    *initial_rot,
                    data.rotation_centroid,
                    delta
                );

                pos_changes.push((*entity, *initial_pos, new_pos));
                rot_changes.push((*entity, Quat::from_rotation_z(*initial_rot), Quat::from_rotation_z(new_rot)));

                // Restore Dynamic
                 ev_prop.send(PropertyChangeEvent {
                    entity: *entity,
                    change: PropertyChange::RigidBody(RigidBody::Dynamic),
                 });
             }

             ev_commit.send(CommitDragEvent {
                 position_changes: pos_changes,
                 rotation_changes: rot_changes,
             });
        }
        data.is_rotating = false;
        data.initial_state.clear();
    }
}

fn is_point_in_box(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

fn calculate_rotation_update(
    initial_pos: Vec2,
    initial_rot: f32,
    centroid: Vec2,
    delta: Vec2,
) -> (Vec2, f32) {
    let angle_delta = delta.y * 0.01;
    let new_rot = initial_rot + angle_delta;

    let relative = initial_pos - centroid;
    let cos = angle_delta.cos();
    let sin = angle_delta.sin();
    let rotated_rel = Vec2::new(
        relative.x * cos - relative.y * sin,
        relative.x * sin + relative.y * cos,
    );
    let new_pos = centroid + rotated_rel;

    (new_pos, new_rot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::f32::consts::PI;

    #[rstest]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(5.0, 5.0), true)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(-5.0, 5.0), false)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(15.0, 5.0), false)]
    #[case(Vec2::ZERO, Vec2::new(10.0, 10.0), Vec2::new(5.0, 15.0), false)]
    fn test_box_selection_logic(
        #[case] start: Vec2,
        #[case] end: Vec2,
        #[case] point: Vec2,
        #[case] expected: bool,
    ) {
        let min = start.min(end);
        let max = start.max(end);
        let contained = is_point_in_box(point, min, max);
        assert_eq!(contained, expected);
    }

    #[rstest]
    #[case(
        HitSortInfo { is_ground: false, z_index: 10.0 },
        HitSortInfo { is_ground: true, z_index: 20.0 },
        std::cmp::Ordering::Less // false < true => Less => Non-ground first
    )]
    #[case(
        HitSortInfo { is_ground: true, z_index: 20.0 },
        HitSortInfo { is_ground: false, z_index: 10.0 },
        std::cmp::Ordering::Greater
    )]
    #[case(
        HitSortInfo { is_ground: false, z_index: 20.0 },
        HitSortInfo { is_ground: false, z_index: 10.0 },
        std::cmp::Ordering::Less
    )]
    #[case(
        HitSortInfo { is_ground: false, z_index: 10.0 },
        HitSortInfo { is_ground: false, z_index: 20.0 },
        std::cmp::Ordering::Greater
    )]
    #[case(
        HitSortInfo { is_ground: false, z_index: 10.0 },
        HitSortInfo { is_ground: false, z_index: 10.0 },
        std::cmp::Ordering::Equal
    )]
    fn test_compare_hits(
        #[case] a: HitSortInfo,
        #[case] b: HitSortInfo,
        #[case] expected: std::cmp::Ordering,
    ) {
        assert_eq!(compare_hits(&a, &b), expected);
    }

    #[rstest]
    fn test_calculate_rotation_update_no_movement() {
        let initial_pos = Vec2::new(10.0, 0.0);
        let initial_rot = 0.0;
        let centroid = Vec2::ZERO;
        let delta = Vec2::ZERO;

        let (pos, rot) = calculate_rotation_update(initial_pos, initial_rot, centroid, delta);

        assert_eq!(pos, initial_pos);
        assert_eq!(rot, initial_rot);
    }

    #[rstest]
    fn test_calculate_rotation_update_90_degrees() {
        let initial_pos = Vec2::new(10.0, 0.0);
        let initial_rot = 0.0;
        let centroid = Vec2::ZERO;

        let delta = Vec2::new(0.0, PI / 2.0 * 100.0);

        let (pos, rot) = calculate_rotation_update(initial_pos, initial_rot, centroid, delta);

        assert!((rot - PI / 2.0).abs() < 1e-5);
        assert!((pos.x - 0.0).abs() < 1e-5);
        assert!((pos.y - 10.0).abs() < 1e-5);
    }
}
