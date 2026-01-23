//! Unified tool for creating joints (Hinges, Fixes, Sliders, Springs, Ropes).
//!
//! Handles creation of various joint types with consistent behavior:
//! - Snaps to grid.
//! - Handles single body (pin to world) and two bodies (connect).
//! - Supports click-to-connect (overlapping bodies) and drag-to-connect (separated bodies).
//! - Spawns visual indicators.

use crate::input::events::{JointType, SpawnJointEvent};
use crate::input::tools::utils::{calculate_local_anchor, is_pointer_over_ui};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use bevy::math::Vec2;
use bevy_egui::EguiContexts;
use std::cmp::Ordering;

/// Plugin for the Connector Tool.
pub struct ConnectorToolPlugin;

impl Plugin for ConnectorToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                update_connector.run_if(|state: Res<State<ToolState>>| {
                    matches!(
                        state.get(),
                        ToolState::RevoluteJoint
                            | ToolState::Weld
                            | ToolState::PrismaticJoint
                            | ToolState::SpringJoint
                            | ToolState::RopeJoint
                    )
                }),
                update_connector_visuals,
            ),
        );
    }
}

/// Component that links a visual indicator to a joint's anchors.
///
/// Ensures the visual stays at the midpoint of the anchors even if the joint stretches.
#[derive(Component)]
pub struct Connector {
    /// The primary body (parent of the visual).
    pub entity_a: Entity,
    /// The secondary body (optional).
    pub entity_b: Option<Entity>,
    /// Local anchor on A.
    pub local_anchor_a: Vec2,
    /// Local anchor on B.
    pub local_anchor_b: Vec2,
}

fn update_connector_visuals(
    mut visuals: Query<(&mut Transform, &Connector, &Parent)>,
    global_transforms: Query<&GlobalTransform>,
) {
    for (mut transform, connector, parent) in &mut visuals {
        if let Ok(parent_global) = global_transforms.get(parent.get()) {
            // Calculate world anchors
            let anchor_a_world = if let Ok(t_a) = global_transforms.get(connector.entity_a) {
                let t = t_a.compute_transform();
                t.transform_point(Vec3::new(
                    connector.local_anchor_a.x,
                    connector.local_anchor_a.y,
                    0.0,
                ))
            } else {
                continue;
            };

            let anchor_b_world = if let Some(e_b) = connector.entity_b {
                if let Ok(t_b) = global_transforms.get(e_b) {
                    let t = t_b.compute_transform();
                    t.transform_point(Vec3::new(
                        connector.local_anchor_b.x,
                        connector.local_anchor_b.y,
                        0.0,
                    ))
                } else {
                    anchor_a_world
                }
            } else {
                Vec3::new(connector.local_anchor_b.x, connector.local_anchor_b.y, 0.0)
            };

            let midpoint = (anchor_a_world + anchor_b_world) * 0.5;

            let parent_inv = parent_global.affine().inverse();
            let local_midpoint = parent_inv.transform_point3(midpoint);

            transform.translation.x = local_midpoint.x;
            transform.translation.y = local_midpoint.y;
        }
    }
}

/// Type of connector (joint) to create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorType {
    /// A revolute joint (hinge).
    Hinge,
    /// A fixed joint (weld).
    Fix,
    /// A prismatic joint (slider).
    Prismatic,
    /// A spring joint (distance).
    Spring,
    /// A rope joint.
    Rope,
}

impl ConnectorType {
    fn from_tool_state(state: &ToolState) -> Option<Self> {
        match state {
            ToolState::RevoluteJoint => Some(Self::Hinge),
            ToolState::Weld => Some(Self::Fix),
            ToolState::PrismaticJoint => Some(Self::Prismatic),
            ToolState::SpringJoint => Some(Self::Spring),
            ToolState::RopeJoint => Some(Self::Rope),
            _ => None,
        }
    }
}

/// State for tracking drag operations in the connector tool.
#[derive(Default, Debug, Clone)]
pub struct ConnectorDragState {
    /// The world position where the drag started.
    pub start_pos: Option<Vec2>,
    /// The entity under the cursor when the drag started.
    pub start_entity: Option<Entity>,
}

/// Action to be taken by the system based on logic result.
#[derive(Debug, PartialEq)]
pub enum ConnectorToolAction {
    /// No action.
    None,
    /// Update drag preview visual.
    UpdateDragPreview {
        /// Start point of the drag.
        start: Vec2,
        /// Current end point of the drag.
        end: Vec2,
    },
    /// Spawn a joint.
    SpawnJoint {
        /// The type of joint to spawn.
        connector_type: ConnectorType,
        /// The first entity involved (if any).
        entity_a: Option<Entity>,
        /// The second entity involved (if any).
        entity_b: Option<Entity>,
        /// The anchor point for the first entity in world space.
        anchor_a_world: Vec2,
        /// The anchor point for the second entity in world space.
        anchor_b_world: Vec2,
        /// Whether the interaction was a drag (vs a click).
        is_drag: bool,
    },
}

/// Pure logic for connector tool input handling.
pub fn handle_connector_input_logic(
    cursor_pos: Option<Vec2>,
    mouse_just_pressed: bool,
    mouse_pressed: bool,
    mouse_just_released: bool,
    tool_state: &ToolState,
    is_pointer_over_ui: bool,
    grid_show: bool,
    grid_snap: bool,
    grid_spacing: f32,
    bodies_under_cursor: &[Entity],
    drag_state: &mut ConnectorDragState,
) -> ConnectorToolAction {
    if is_pointer_over_ui {
        return ConnectorToolAction::None;
    }

    let Some(raw_pos) = cursor_pos else {
        return ConnectorToolAction::None;
    };

    let Some(connector_type) = ConnectorType::from_tool_state(tool_state) else {
        return ConnectorToolAction::None;
    };

    let pos = if grid_show && grid_snap {
        snap_to_grid(raw_pos, grid_spacing)
    } else {
        raw_pos
    };

    if mouse_just_pressed {
        drag_state.start_pos = Some(pos);
        drag_state.start_entity = bodies_under_cursor.first().cloned();
    }

    if mouse_pressed {
        if let Some(start_pos) = drag_state.start_pos {
            return ConnectorToolAction::UpdateDragPreview {
                start: start_pos,
                end: pos,
            };
        }
    }

    if mouse_just_released {
        if let Some(start_pos) = drag_state.start_pos {
            let end_pos = pos;
            let bodies_at_end = bodies_under_cursor;
            let end_entity = bodies_at_end.first().cloned();

            let dist = start_pos.distance(end_pos);
            let is_drag = dist > 0.1;

            let mut entity_a = None;
            let mut entity_b = None;
            let mut anchor_a_world = start_pos;
            let mut anchor_b_world = end_pos;

            if is_drag {
                entity_a = drag_state.start_entity;
                entity_b = end_entity;
            } else {
                // Click behavior: Overlapping at start_pos
                // NOTE: When clicking, the 'bodies_under_cursor' passed to this function
                // are essentially the bodies at 'start_pos' (since pos ~ start_pos).
                if !bodies_at_end.is_empty() {
                    entity_a = Some(bodies_at_end[0]);
                    entity_b = bodies_at_end.get(1).cloned();
                }
                anchor_a_world = start_pos;
                anchor_b_world = start_pos;
            }

            // Clean up: prevent self-connection
            if let (Some(a), Some(b)) = (entity_a, entity_b) {
                if a == b {
                    entity_b = None;
                }
            }

            // Swap if A is missing but B is present
            if entity_a.is_none() && entity_b.is_some() {
                entity_a = entity_b;
                entity_b = None;
                std::mem::swap(&mut anchor_a_world, &mut anchor_b_world);
            }

            // Only spawn if we have at least one entity or it's a valid world-to-world interaction
            // (though usually we need at least one body for a joint, except maybe rope between two static points?
            // But Rapier joints usually need a body. Pin to world means one body.
            // If entity_a is None, we can't create a joint.)
            if entity_a.is_some() {
                // Reset drag state
                drag_state.start_pos = None;
                drag_state.start_entity = None;

                return ConnectorToolAction::SpawnJoint {
                    connector_type,
                    entity_a,
                    entity_b,
                    anchor_a_world,
                    anchor_b_world,
                    is_drag,
                };
            }
        }
        drag_state.start_pos = None;
        drag_state.start_entity = None;
    }

    ConnectorToolAction::None
}

fn update_connector(
    mut event_writer: EventWriter<SpawnJointEvent>,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
    tool_state: Res<State<ToolState>>,
    bodies: Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: Query<&Parent>,
    transforms: Query<&Transform>,
    mut drag_state: Local<ConnectorDragState>,
    mut gizmos: Gizmos,
) {
    // Helper to resolve list of intersected entities
    // We do this outside the logic function because it requires ECS queries.
    let bodies_under_cursor = if let Some(raw_pos) = cursor_pos.0 {
         let pos = if grid_settings.show && grid_settings.snap {
            snap_to_grid(raw_pos, grid_settings.spacing)
        } else {
            raw_pos
        };

        if let Some(rapier_context) = rapier_context_query.iter().next() {
            let shape = Collider::ball(0.1);
            let filter = QueryFilter::default().exclude_sensors();
            let mut intersections = Vec::new();
            rapier_context.intersections_with_shape(pos, 0.0, &shape, filter, |e| {
                intersections.push(e);
                true
            });
            resolve_sorted_bodies(&intersections, &bodies, &parents)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let action = handle_connector_input_logic(
        cursor_pos.0,
        mouse.just_pressed(MouseButton::Left),
        mouse.pressed(MouseButton::Left),
        mouse.just_released(MouseButton::Left),
        tool_state.get(),
        is_pointer_over_ui(&mut contexts),
        grid_settings.show,
        grid_settings.snap,
        grid_settings.spacing,
        &bodies_under_cursor,
        &mut drag_state,
    );

    match action {
        ConnectorToolAction::None => {},
        ConnectorToolAction::UpdateDragPreview { start, end } => {
             gizmos.line_2d(start, end, Color::srgb(1.0, 1.0, 0.0));
        },
        ConnectorToolAction::SpawnJoint { connector_type, entity_a, entity_b, anchor_a_world, anchor_b_world, is_drag } => {
            if let Some(e_a) = entity_a {
                let get_local = |e: Entity, p: Vec2| -> Vec2 {
                    if let Ok(t) = transforms.get(e) {
                        calculate_local_anchor(t, p)
                    } else {
                        p
                    }
                };
                let get_rot = |e: Entity| -> f32 {
                     if let Ok(t) = transforms.get(e) {
                         t.rotation.to_euler(EulerRot::XYZ).2
                     } else {
                         0.0
                     }
                };

                let anchor_a = get_local(e_a, anchor_a_world);
                let anchor_b = if let Some(e_b) = entity_b {
                    get_local(e_b, anchor_b_world)
                } else {
                    anchor_b_world
                };

                let rot_a = get_rot(e_a);
                let rot_b = if let Some(e_b) = entity_b { get_rot(e_b) } else { 0.0 };

                match connector_type {
                    ConnectorType::Hinge => {
                        event_writer.send(SpawnJointEvent {
                            joint_type: JointType::Revolute,
                            entity_a: e_a,
                            entity_b,
                            anchor_a,
                            anchor_b,
                        });
                    }
                    ConnectorType::Fix => {
                        event_writer.send(SpawnJointEvent {
                            joint_type: JointType::Fixed { rot_a, rot_b },
                            entity_a: e_a,
                            entity_b,
                            anchor_a,
                            anchor_b,
                        });
                    }
                    ConnectorType::Prismatic => {
                        let axis_world = if is_drag {
                            let dir = (anchor_b_world - anchor_a_world).normalize_or_zero();
                            if dir == Vec2::ZERO {
                                Vec2::Y
                            } else {
                                dir
                            }
                        } else {
                            Vec2::Y
                        };
                        // Convert axis to local space of A
                        let axis = axis_world.rotate(Vec2::from_angle(-rot_a));

                        event_writer.send(SpawnJointEvent {
                            joint_type: JointType::Prismatic { axis },
                            entity_a: e_a,
                            entity_b,
                            anchor_a,
                            anchor_b,
                        });
                    }
                    ConnectorType::Spring => {
                        let length = if is_drag {
                            anchor_a_world.distance(anchor_b_world)
                        } else {
                            1.0
                        };
                        event_writer.send(SpawnJointEvent {
                            joint_type: JointType::Spring {
                                stiffness: 10.0,
                                damping: 0.5,
                                length,
                            },
                            entity_a: e_a,
                            entity_b,
                            anchor_a,
                            anchor_b,
                        });
                    }
                    ConnectorType::Rope => {
                        let length = if is_drag {
                            anchor_a_world.distance(anchor_b_world)
                        } else {
                            5.0
                        };
                        event_writer.send(SpawnJointEvent {
                            joint_type: JointType::Rope { length },
                            entity_a: e_a,
                            entity_b,
                            anchor_a,
                            anchor_b,
                        });
                    }
                }
            }
        },
    }
}

/// Resolves a list of intersected entities (likely colliders) to their root RigidBody ancestors,
/// deduplicates them, and sorts them by Z-index (Descending: Top first).
fn resolve_sorted_bodies(
    intersections: &[Entity],
    bodies: &Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: &Query<&Parent>,
) -> Vec<Entity> {
    let mut resolved = Vec::new();

    for &hit_entity in intersections {
        // Traverse up to find RigidBody
        let mut current = hit_entity;
        loop {
            if let Ok((entity, global_transform)) = bodies.get(current) {
                resolved.push((entity, global_transform.translation().z));
                break;
            }
            // Move up
            if let Ok(parent) = parents.get(current) {
                current = parent.get();
            } else {
                break; // No rigid body ancestor
            }
        }
    }

    // Deduplicate
    resolved.sort_by_key(|k| k.0);
    resolved.dedup_by_key(|k| k.0);

    // Sort by Z (Descending)
    sort_bodies_by_z(&mut resolved);

    resolved.into_iter().map(|(e, _)| e).collect()
}

fn sort_bodies_by_z(bodies: &mut Vec<(Entity, f32)>) {
    bodies.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(ToolState::RevoluteJoint, Some(ConnectorType::Hinge))]
    #[case(ToolState::Weld, Some(ConnectorType::Fix))]
    #[case(ToolState::PrismaticJoint, Some(ConnectorType::Prismatic))]
    #[case(ToolState::SpringJoint, Some(ConnectorType::Spring))]
    #[case(ToolState::RopeJoint, Some(ConnectorType::Rope))]
    #[case(ToolState::Select, None)]
    fn test_connector_type_from_state(
        #[case] state: ToolState,
        #[case] expected: Option<ConnectorType>,
    ) {
        assert_eq!(ConnectorType::from_tool_state(&state), expected);
    }

    #[test]
    fn test_sort_bodies_by_z() {
        let e1 = Entity::from_raw(1);
        let e2 = Entity::from_raw(2);
        let e3 = Entity::from_raw(3);

        let mut bodies = vec![
            (e1, 1.0), // Low
            (e2, 5.0), // High
            (e3, 2.0), // Mid
        ];

        sort_bodies_by_z(&mut bodies);

        assert_eq!(bodies[0].0, e2);
        assert_eq!(bodies[1].0, e3);
        assert_eq!(bodies[2].0, e1);
    }

    #[rstest]
    #[case::click_connect_overlapping(
        Some(Vec2::new(10.0, 10.0)),
        true, false, true, // just_pressed, pressed, just_released
        ToolState::RevoluteJoint,
        vec![Entity::from_raw(1), Entity::from_raw(2)],
        ConnectorToolAction::SpawnJoint {
            connector_type: ConnectorType::Hinge,
            entity_a: Some(Entity::from_raw(1)),
            entity_b: Some(Entity::from_raw(2)),
            anchor_a_world: Vec2::new(10.0, 10.0),
            anchor_b_world: Vec2::new(10.0, 10.0),
            is_drag: false,
        }
    )]
    #[case::drag_connect_separated(
        Some(Vec2::new(20.0, 20.0)), // End Pos
        false, false, true,
        ToolState::SpringJoint,
        vec![Entity::from_raw(2)], // Body at end
        ConnectorToolAction::SpawnJoint {
            connector_type: ConnectorType::Spring,
            entity_a: Some(Entity::from_raw(1)), // From setup (drag state)
            entity_b: Some(Entity::from_raw(2)),
            anchor_a_world: Vec2::new(10.0, 10.0),
            anchor_b_world: Vec2::new(20.0, 20.0),
            is_drag: true,
        }
    )]
    #[case::drag_to_void_pin(
        Some(Vec2::new(20.0, 20.0)),
        false, false, true,
        ToolState::RevoluteJoint,
        vec![], // No body at end
        ConnectorToolAction::SpawnJoint {
            connector_type: ConnectorType::Hinge,
            entity_a: Some(Entity::from_raw(1)),
            entity_b: None,
            anchor_a_world: Vec2::new(10.0, 10.0),
            anchor_b_world: Vec2::new(20.0, 20.0),
            is_drag: true,
        }
    )]
    #[case::self_connect_prevention(
        Some(Vec2::new(20.0, 20.0)),
        false, false, true,
        ToolState::RevoluteJoint,
        vec![Entity::from_raw(1)], // End on same body
        ConnectorToolAction::SpawnJoint {
            connector_type: ConnectorType::Hinge,
            entity_a: Some(Entity::from_raw(1)),
            entity_b: None, // Cleared
            anchor_a_world: Vec2::new(10.0, 10.0),
            anchor_b_world: Vec2::new(20.0, 20.0),
            is_drag: true,
        }
    )]
    fn test_handle_connector_input_logic(
        #[case] cursor_pos: Option<Vec2>,
        #[case] mouse_just_pressed: bool,
        #[case] mouse_pressed: bool,
        #[case] mouse_just_released: bool,
        #[case] tool_state: ToolState,
        #[case] bodies_under_cursor: Vec<Entity>,
        #[case] expected_action: ConnectorToolAction,
    ) {
        let mut drag_state = ConnectorDragState::default();

        // Setup initial drag state for drag cases
        if !mouse_just_pressed && (mouse_pressed || mouse_just_released) {
            drag_state.start_pos = Some(Vec2::new(10.0, 10.0));
            drag_state.start_entity = Some(Entity::from_raw(1));
        }

        // For click case (all true for simplicity in parametric definition,
        // though strictly they happen in sequence frames.
        // Here we simulate the 'release' frame primarily for Spawn results.
        // For 'click', existing code treats it as a drag < 0.1 dist.
        // So for the click test case, we assume we are in the Release frame,
        // and start_pos was set previously.
        // BUT wait, existing code sets start_pos on JustPressed.
        // If we are testing JustReleased, start_pos must be there.
        // My setup block above handles that.

        let action = handle_connector_input_logic(
            cursor_pos,
            mouse_just_pressed,
            mouse_pressed,
            mouse_just_released,
            &tool_state,
            false, // is_pointer_over_ui
            false, // grid_show
            false, // grid_snap
            1.0,   // grid_spacing
            &bodies_under_cursor,
            &mut drag_state,
        );

        assert_eq!(action, expected_action);
    }
}
