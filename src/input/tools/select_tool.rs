//! Tool for selecting entities.
//!
//! Simply allows clicking on entities to populate the `Selection` resource.

use crate::input::editable::{EditableBox, EditableCircle};
use crate::input::selection::{NextGroupID, Selection, SelectionFilter, SelectionGroup};
use crate::input::tools::connector::Connector;
use crate::input::tools::utils::is_pointer_over_ui;
use bevy_prototype_lyon::prelude::{Fill, Stroke};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::physics::floor::GroundPlane;
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy_egui::EguiContexts;
use bevy::ecs::system::SystemParam;

/// Auxiliary queries used by the select tool.
#[derive(SystemParam)]
pub struct AuxQueries<'w, 's> {
    /// Selectable entities.
    pub selectable: Query<'w, 's, (Entity, &'static GlobalTransform), (With<Collider>, Without<GroundPlane>)>,
    /// Connectors.
    pub connector: Query<'w, 's, &'static Connector>,
    /// Selection groups.
    pub group: Query<'w, 's, (Entity, &'static SelectionGroup)>,
    /// Drag preview entities.
    pub drag_preview: Query<'w, 's, (Entity, &'static DragPreview)>,
    /// Mesh and Material.
    pub mesh: Query<'w, 's, (Option<&'static Mesh3d>, Option<&'static MeshMaterial3d<StandardMaterial>>)>,
}

/// Information needed to sort hits for selection.
#[derive(Debug, PartialEq)]
pub struct HitSortInfo {
    /// Whether the entity is a ground plane (should be prioritized lower than objects).
    pub is_ground: bool,
    /// The Z-index of the entity (higher means closer to camera, thus higher priority).
    pub z_index: f32,
}

/// Compare two hits for selection priority.
///
/// Priority:
/// 1. Non-ground entities come first.
/// 2. Higher Z-index comes first.
pub fn compare_hits(a: &HitSortInfo, b: &HitSortInfo) -> std::cmp::Ordering {
    // Non-ground (is_ground = false) < Ground (is_ground = true)
    // We want Non-ground FIRST.
    // false < true.
    let ground_order = a.is_ground.cmp(&b.is_ground);
    if ground_order != std::cmp::Ordering::Equal {
        return ground_order;
    }

    // Z index: Higher is better (comes first).
    // Sort descending: b.cmp(a)
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

/// Component marking an entity as a preview during drag (copy operation).
#[derive(Component)]
pub struct DragPreview {
    /// The original RigidBody component (to restore after drag).
    pub original_rb: Option<RigidBody>,
    /// Whether the entity was a sensor before drag.
    pub was_sensor: bool,
}

/// Runtime data for the Select Tool.
#[derive(Resource, Default)]
pub struct SelectToolData {
    /// Start position of the drag box.
    pub drag_start: Option<Vec2>,
    /// Whether entities are being moved.
    pub is_moving: bool,
    /// Store offsets for multiple entities: Map Entity -> Initial Vec2
    pub initial_positions: Vec<(Entity, Vec2)>,
    /// Start position of the drag move.
    pub drag_start_pos: Vec2,

    /// Whether entities are being rotated.
    pub is_rotating: bool,
    /// Start position of the rotation drag.
    pub rotate_start_pos: Vec2,
    /// Initial rotations and positions: Entity, Rotation, Position
    pub initial_rotations: Vec<(Entity, f32, Vec2)>,
    /// Centroid of the rotation.
    pub rotation_centroid: Vec2,
}

fn select_tool_reset(mut data: ResMut<SelectToolData>) {
    *data = SelectToolData::default();
}

fn select_tool_update(
    mut selection: ResMut<Selection>,
    selection_filter: Res<SelectionFilter>,
    mut data: ResMut<SelectToolData>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
    mut gizmos: Gizmos,
    mut queries: ParamSet<(
        Query<(Entity, &mut Transform, Option<&GroundPlane>)>,
        Query<(
            &Transform,
            &Collider,
            Option<&RigidBody>,
            Option<&EditableBox>,
            Option<&EditableCircle>,
            Option<&Fill>,
            Option<&Stroke>,
            Option<&Friction>,
            Option<&Restitution>,
            Option<&ColliderMassProperties>,
            Option<&GravityScale>,
            Option<&LockedAxes>,
            Option<&Sensor>,
            Option<&SelectionGroup>,
        )>,
    )>,
    aux: AuxQueries,
    grid_settings: Res<GridSettings>,
    mut next_group_id: ResMut<NextGroupID>,
    mut commands: Commands,
) {
    // Prevent selection if over UI
    if is_pointer_over_ui(&mut contexts)
        && !data.is_moving
        && !data.is_rotating
        && data.drag_start.is_none()
    {
        return;
    }

    let Some(current_pos) = cursor_pos.0 else {
        return;
    };

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    if mouse.just_pressed(MouseButton::Left) {
        data.drag_start_pos = current_pos;

        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };
        let filter = QueryFilter::default().exclude_sensors();

        // Collect all hits
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

        // Sort hits: Non-ground first, then by Z index (descending)
        let query0 = queries.p0();
        hits.sort_by(|&a, &b| {
            let get_info = |entity| -> Option<HitSortInfo> {
                if let Ok((_, t, g)) = query0.get(entity) {
                    Some(HitSortInfo {
                        is_ground: g.is_some(),
                        z_index: t.translation.z,
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

        if let Some(&entity) = hits.first() {
            // Clicked on something

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

            // If shift not held and entity not in selection, clear selection
            let clicked_already_selected = selection.0.contains(&entity);
            if !shift && !clicked_already_selected {
                selection.clear();
            }

            if shift {
                // Toggle group
                let any_selected = entities_to_select.iter().any(|e| selection.0.contains(e));
                if any_selected {
                    for e in entities_to_select {
                        selection.remove(e);
                    }
                } else {
                    for e in entities_to_select {
                        selection.add(e);
                    }
                }
            } else if !clicked_already_selected {
                for e in entities_to_select {
                    selection.add(e);
                }
            }

            // Copy on Drag
            if ctrl && selection.0.contains(&entity) {
                // Duplicate selected entities
                let mut new_selection = Vec::new();
                let mut group_map = std::collections::HashMap::new();

                for &old_entity in &selection.0 {
                    if let Ok((
                        t,
                        collider,
                        rb,
                        ebox,
                        ecircle,
                        fill,
                        stroke,
                        friction,
                        restitution,
                        mass,
                        gravity,
                        locked,
                        sensor,
                        group,
                    )) = queries.p1().get(old_entity) {
                        let mut builder = commands.spawn((
                            *t,
                            collider.clone(),
                        ));

                        if let Some(c) = rb { builder.insert(*c); }
                        if let Some(c) = ebox { builder.insert(*c); }
                        if let Some(c) = ecircle { builder.insert(*c); }
                        if let Some(c) = fill { builder.insert(c.clone()); }
                        if let Some(c) = stroke { builder.insert(c.clone()); }
                        if let Some(c) = friction { builder.insert(*c); }
                        if let Some(c) = restitution { builder.insert(*c); }
                        if let Some(c) = mass { builder.insert(c.clone()); }
                        if let Some(c) = gravity { builder.insert(*c); }
                        if let Some(c) = locked { builder.insert(*c); }
                        if let Some(_) = sensor { builder.insert(Sensor); }

                        if let Ok((mesh, material)) = aux.mesh.get(old_entity) {
                            if let Some(c) = mesh { builder.insert(c.clone()); }
                            if let Some(c) = material { builder.insert(c.clone()); }
                        }

                        // Group Logic
                        if let Some(g) = group {
                             let new_id = *group_map.entry(g.0).or_insert_with(|| {
                                 let id = next_group_id.0;
                                 next_group_id.0 += 1;
                                 id
                             });
                             builder.insert(SelectionGroup(new_id));
                        }

                        // Drag Preview: Set as Kinematic and Sensor to prevent interaction during drag
                        builder.insert(DragPreview {
                            original_rb: rb.copied(),
                            was_sensor: sensor.is_some(),
                        });
                        builder.insert(RigidBody::KinematicPositionBased);
                        builder.insert(Sensor);

                        // Disable sleeping so it can move
                        builder.insert(Sleeping::disabled());

                        new_selection.push(builder.id());
                    }
                }

                if !new_selection.is_empty() {
                    selection.clear();
                    for e in new_selection {
                        selection.add(e);
                    }
                    info!("Duplicated {} entities", selection.0.len());
                }
            }

            // Initiate Move for all selected
            if selection.0.contains(&entity) {
                data.is_moving = true;
                data.initial_positions.clear();
                let q0 = queries.p0();
                for &entity in &selection.0 {
                    if let Ok((_, t, _)) = q0.get(entity) {
                        data.initial_positions
                            .push((entity, t.translation.truncate()));
                    }
                }
            }
        } else {
            // Clicked on empty space -> Box Select
            if !shift {
                selection.clear();
            }
            data.is_moving = false;
            data.drag_start = Some(current_pos);
        }
    }

    if mouse.pressed(MouseButton::Left) {
        if data.is_moving {
            // Move the entities
            let delta = current_pos - data.drag_start_pos;
            let mut q0 = queries.p0();

            for (entity, initial_pos) in &data.initial_positions {
                if let Ok((_, mut t, _)) = q0.get_mut(*entity) {
                    let mut new_pos = *initial_pos + delta;
                    if grid_settings.show && grid_settings.snap {
                        new_pos = snap_to_grid(new_pos, grid_settings.spacing);
                    }
                    t.translation.x = new_pos.x;
                    t.translation.y = new_pos.y;
                }
            }
        } else if let Some(start) = data.drag_start {
            // Draw Box
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;
            let center = (min + max) / 2.0;

            gizmos.rect_2d(
                Isometry2d::from_translation(Vec2::new(center.x, center.y)),
                Vec2::new(size.x, size.y),
                Color::srgb(0.0, 1.0, 1.0),
            );
        }
    }

    if mouse.just_released(MouseButton::Left) {
        if !data.is_moving
            && let Some(start) = data.drag_start
        {
            // Box Select Finalize
            let min = start.min(current_pos);
            let max = start.max(current_pos);
            let size = max - min;

            if size.x > 0.1 && size.y > 0.1 {
                let mut count = 0;
                // Manual AABB check against all selectable entities
                for (entity, global_transform) in &aux.selectable {
                    let t = global_transform.translation().truncate();
                    if is_point_in_box(t, min, max) {
                        // Insert directly to avoid spamming "Added entity" logs
                        if selection.0.insert(entity) {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    info!("Select Tool: Box Selected {} entities", count);
                }
            }
        }

        if data.is_moving {
            info!("Select Tool: Moved {} entities", selection.0.len());

            // Restore DragPreview entities (copied objects)
            for (entity, preview) in &aux.drag_preview {
                if let Some(rb) = preview.original_rb {
                    commands.entity(entity).insert(rb);
                } else {
                    commands.entity(entity).remove::<RigidBody>();
                }

                if !preview.was_sensor {
                    commands.entity(entity).remove::<Sensor>();
                }

                commands.entity(entity).remove::<DragPreview>();
                commands.entity(entity).insert(Sleeping::disabled());
            }
        }

        data.is_moving = false;
        data.drag_start = None;
        data.initial_positions.clear();
    }

    // Right Click Rotation
    if mouse.just_pressed(MouseButton::Right) && !selection.0.is_empty() {
        // Only start rotation if cursor is over a selected entity
        let mut pointer_over_selection = false;
        if let Some(rapier_context) = rapier_context_query.iter().next() {
            let filter = QueryFilter::default().exclude_sensors();
             rapier_context.intersections_with_point(current_pos, filter, |entity| {
                if selection.0.contains(&entity) {
                    pointer_over_selection = true;
                    false // Stop search
                } else {
                    true
                }
            });
        }

        if pointer_over_selection {
            data.is_rotating = true;
            data.rotate_start_pos = current_pos;
            data.initial_rotations.clear();

            let mut centroid = Vec2::ZERO;
            let mut count = 0.0;
            let q0 = queries.p0();

            for &entity in &selection.0 {
                if let Ok((_, t, _)) = q0.get(entity) {
                    let rot = t.rotation.to_euler(EulerRot::XYZ).2;
                    let pos = t.translation.truncate();
                    data.initial_rotations.push((entity, rot, pos));
                    centroid += pos;
                    count += 1.0;
                }
            }

            if count > 0.0 {
                data.rotation_centroid = centroid / count;
            }
        }
    }

    if mouse.pressed(MouseButton::Right) && data.is_rotating {
        let delta = current_pos - data.rotate_start_pos;
        // Sensitivity: 100 pixels = 1 radian approx? Or just delta.y
        let angle_delta = delta.y * 0.01;

        let mut q0 = queries.p0();
        for (entity, initial_rot, initial_pos) in &data.initial_rotations {
            if let Ok((_, mut t, _)) = q0.get_mut(*entity) {
                // Rotate rotation
                let new_rot = initial_rot + angle_delta;
                t.rotation = Quat::from_rotation_z(new_rot);

                // Rotate position around centroid
                let relative = *initial_pos - data.rotation_centroid;
                // Rotate vector
                let cos = angle_delta.cos();
                let sin = angle_delta.sin();
                let rotated_rel = Vec2::new(
                    relative.x * cos - relative.y * sin,
                    relative.x * sin + relative.y * cos,
                );

                let new_pos = data.rotation_centroid + rotated_rel;
                t.translation.x = new_pos.x;
                t.translation.y = new_pos.y;
            }
        }
    }

    if mouse.just_released(MouseButton::Right) {
        data.is_rotating = false;
        data.initial_rotations.clear();
    }
}

fn is_point_in_box(point: Vec2, min: Vec2, max: Vec2) -> bool {
    point.x >= min.x && point.x <= max.x && point.y >= min.y && point.y <= max.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

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
        std::cmp::Ordering::Less // Higher Z first => a > b in value, but we sort descending so compare(b, a) => Greater?
        // Wait. b.cmp(a).
        // 20.cmp(10) is Greater.
        // Wait, descending sort means Higher should be "Less" (come before)?
        // No, in Rust sort, Less means "comes before".
        // If we want Descending (High -> Low), then compare(High, Low) should return Less.
        // compare_hits returns b.z_index.cmp(a.z_index).
        // Case: a=20, b=10.
        // b.cmp(a) -> 10.cmp(20) -> Less.
        // So compare_hits(High, Low) returns Less.
        // Correct.
    )]
    #[case(
        HitSortInfo { is_ground: false, z_index: 10.0 },
        HitSortInfo { is_ground: false, z_index: 20.0 },
        std::cmp::Ordering::Greater // 20.cmp(10) -> Greater
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
}
