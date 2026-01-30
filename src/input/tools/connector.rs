//! Unified tool for creating joints (Hinges and Fixes).
//!
//! Handles creation of `RevoluteJoint` and `FixedJoint` with consistent behavior:
//! - Snaps to grid.
//! - Handles single body (pin to world) and two bodies (connect).
//! - Sorts overlapping bodies by Z-depth.
//! - Spawns visual indicators parented to the bodies.
//!
//! # TODO
//! - **Visuals**: Currently uses basic debug shapes. Replace with proper sprites/meshes (Bolts, Welds).
//! - **Limits**: Expose joint limits (angle limits, motor data) in the UI and pass them to the commands.
//! - **Breakage**: Implement breakable joints (impulse threshold).

use crate::input::commands::{
    CommandStack, SpawnFixedJointCommand, SpawnJointCommand, SpawnPrismaticJointCommand,
};
use crate::input::tools::utils::{calculate_local_anchor, is_pointer_over_ui};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::GridSettings;
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
                        ToolState::RevoluteJoint | ToolState::Weld | ToolState::PrismaticJoint
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

/// Tag component for the moving part of a slider visual.
#[derive(Component)]
pub struct SliderKnob;

fn update_connector_visuals(
    mut visuals: Query<(Entity, &mut Transform, &Connector, &Parent)>,
    mut knobs: Query<(&mut Transform, &Parent), (With<SliderKnob>, Without<Connector>)>,
    global_transforms: Query<&GlobalTransform>,
    rapier_handles: Query<&RapierImpulseJointHandle>,
    rapier_context_query: Query<&RapierContext>,
    joints: Query<&ImpulseJoint>,
) {
    let rapier_context = rapier_context_query.iter().next();

    for (entity, mut transform, connector, parent) in &mut visuals {
        if let Ok(parent_global) = global_transforms.get(parent.get()) {
            // 1. Position: Midpoint of anchors
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

            // 2. Rotation and State Visualization
            let mut joint_angle = 0.0;
            let mut joint_offset = 0.0;

            // Try to get dynamic state from Rapier
            if let Ok(handle) = rapier_handles.get(connector.entity_a) {
                if let Some(ctx) = rapier_context {
                    if let Some(joint) = ctx.impulse_joints.get(handle.0) {
                        if let Some(rev) = joint.data.as_revolute() {
                            if let (Some(b1), Some(b2)) = (ctx.bodies.get(joint.body1), ctx.bodies.get(joint.body2)) {
                                joint_angle = rev.angle(b1.rotation(), b2.rotation());
                            }
                        } else if let Some(pris) = joint.data.as_prismatic() {
                            // Calculate position manually
                            // Use connector entities (A and B/Pin)
                            if let Some(e_b) = connector.entity_b {
                                if let (Ok(b1), Ok(b2)) = (global_transforms.get(connector.entity_a), global_transforms.get(e_b)) {
                                    let anchor1 = pris.local_anchor1();
                                    let anchor2 = pris.local_anchor2();
                                    let axis1 = pris.local_axis1();

                                    let t1 = b1.compute_transform();
                                    let t2 = b2.compute_transform();

                                    let p1 = t1.transform_point(Vec3::new(anchor1.x, anchor1.y, 0.0));
                                    let p2 = t2.transform_point(Vec3::new(anchor2.x, anchor2.y, 0.0));
                                    let axis_world = t1.rotation * Vec3::new(axis1.x, axis1.y, 0.0);

                                    let d = p2 - p1;
                                    joint_offset = d.dot(axis_world);
                                }
                            }
                        }
                    }
                }
            }

            // Determine Base Rotation
            let base_rotation = if connector.entity_b.is_none() {
                // Pinned: Fixed to background -> Inverse of parent rotation
                parent_global.compute_transform().rotation.inverse()
            } else {
                // Connected: Relative to parent -> Identity
                Quat::IDENTITY
            };

            // Determine Axis/Type from ImpulseJoint component to apply state correctly
            if let Ok(joint) = joints.get(connector.entity_a) {
                match &joint.data {
                    TypedJoint::RevoluteJoint(_) => {
                         // Rotate by joint angle
                         transform.rotation = base_rotation * Quat::from_rotation_z(joint_angle);
                    }
                    TypedJoint::PrismaticJoint(data) => {
                         let axis = data.data.local_axis1();
                         let axis_vec = Vec2::new(axis.x, axis.y);
                         let angle = axis_vec.y.atan2(axis_vec.x);

                         transform.rotation = base_rotation * Quat::from_rotation_z(angle);

                         // Update Knob Position
                         for (mut knob_transform, knob_parent) in &mut knobs {
                             if knob_parent.get() == entity {
                                 knob_transform.translation.x = joint_offset;
                             }
                         }
                    }
                    _ => {
                         transform.rotation = base_rotation;
                    }
                }
            } else {
                 transform.rotation = base_rotation;
            }
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
    Slider,
}

impl ConnectorType {
    fn from_tool_state(state: &ToolState) -> Option<Self> {
        match state {
            ToolState::RevoluteJoint => Some(Self::Hinge),
            ToolState::Weld => Some(Self::Fix),
            ToolState::PrismaticJoint => Some(Self::Slider),
            _ => None,
        }
    }
}

fn update_connector(
    mut commands: Commands,
    cursor_pos: Res<CursorWorldPos>,
    mouse: Res<ButtonInput<MouseButton>>,
    rapier_context_query: Query<&RapierContext>,
    mut contexts: EguiContexts,
    _grid_settings: Res<GridSettings>,
    tool_state: Res<State<ToolState>>,
    bodies: Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: Query<&Parent>,
) {
    if is_pointer_over_ui(&mut contexts) {
        return;
    }

    let Some(raw_pos) = cursor_pos.0 else {
        return;
    };

    let Some(connector_type) = ConnectorType::from_tool_state(tool_state.get()) else {
        return;
    };

    if mouse.just_pressed(MouseButton::Left) {
        let pos = raw_pos;

        // Find entities using shape intersection for better robustness
        let Some(rapier_context) = rapier_context_query.iter().next() else {
            return;
        };

        let shape = Collider::ball(0.1);
        let filter = QueryFilter::default().exclude_sensors();
        let mut intersections = Vec::new();

        rapier_context.intersections_with_shape(pos, 0.0, &shape, filter, |e| {
            intersections.push(e);
            true
        });

        // Resolve to bodies and sort
        let sorted_bodies = resolve_sorted_bodies(&intersections, &bodies, &parents);

        if sorted_bodies.is_empty() {
            return;
        }

        let entity_a = sorted_bodies[0];
        let entity_b = if sorted_bodies.len() > 1 {
            Some(sorted_bodies[1])
        } else {
            None
        };

        let get_local_and_rot = |e: Entity| -> (Vec2, f32) {
            if let Ok((_, global_transform)) = bodies.get(e) {
                let t = global_transform.compute_transform();
                let rot = t.rotation.to_euler(EulerRot::XYZ).2;
                (calculate_local_anchor(&t, pos), rot)
            } else {
                (Vec2::ZERO, 0.0)
            }
        };

        let (anchor_a, rot_a) = get_local_and_rot(entity_a);
        let (anchor_b, rot_b) = if let Some(e_b) = entity_b {
            get_local_and_rot(e_b)
        } else {
            (Vec2::ZERO, 0.0)
        };

        match connector_type {
            ConnectorType::Hinge => {
                let cmd = SpawnJointCommand {
                    entity_a,
                    entity_b,
                    anchor_a,
                    anchor_b,
                    compliance: 0.0,
                    visual_entity: None,
                    pin_entity: None,
                    original_solver_groups: None,
                };
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }
            ConnectorType::Fix => {
                let cmd = SpawnFixedJointCommand {
                    entity_a,
                    entity_b,
                    anchor_a,
                    anchor_b,
                    compliance: 0.0,
                    visual_entity: None,
                    pin_entity: None,
                    original_solver_groups: None,
                    rot_a,
                    rot_b,
                };
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }
            ConnectorType::Slider => {
                let cmd = SpawnPrismaticJointCommand {
                    entity_a,
                    entity_b,
                    anchor_a,
                    anchor_b,
                    axis: Vec2::X,
                    compliance: 0.0,
                    visual_entity: None,
                    pin_entity: None,
                    original_solver_groups: None,
                };
                commands.queue(move |world: &mut World| {
                    world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                        stack.push(Box::new(cmd), world);
                    });
                });
            }
        }
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
        if let Some(info) = find_rigidbody_ancestor(
            hit_entity,
            |e| bodies.get(e).map(|(_, t)| t.translation().z).ok(),
            |e| parents.get(e).map(|p| p.get()).ok(),
        ) {
            resolved.push(info);
        }
    }

    // Deduplicate
    resolved.sort_by_key(|k| k.0);
    resolved.dedup_by_key(|k| k.0);

    // Sort by Z (Descending)
    sort_bodies_by_z(&mut resolved);

    resolved.into_iter().map(|(e, _)| e).collect()
}

/// Generic logic to find the first ancestor (or self) that satisfies a condition (is a body),
/// traversing up the hierarchy.
fn find_rigidbody_ancestor<F, G>(
    start_entity: Entity,
    get_body_z: F,
    get_parent: G,
) -> Option<(Entity, f32)>
where
    F: Fn(Entity) -> Option<f32>,    // Returns Some(z) if it is a body
    G: Fn(Entity) -> Option<Entity>, // Returns parent
{
    let mut current = start_entity;
    // Limit depth to prevent infinite loops in cyclic graphs (though Bevy prevents cycles)
    for _ in 0..20 {
        if let Some(z) = get_body_z(current) {
            return Some((current, z));
        }
        if let Some(parent) = get_parent(current) {
            current = parent;
        } else {
            return None;
        }
    }
    None
}

fn sort_bodies_by_z(bodies: &mut [(Entity, f32)]) {
    bodies.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(ToolState::RevoluteJoint, Some(ConnectorType::Hinge))]
    #[case(ToolState::Weld, Some(ConnectorType::Fix))]
    #[case(ToolState::PrismaticJoint, Some(ConnectorType::Slider))]
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

    #[test]
    fn test_find_rigidbody_ancestor() {
        let body = Entity::from_raw(1);
        let child = Entity::from_raw(2);
        let grandchild = Entity::from_raw(3);
        let orphan = Entity::from_raw(4);

        // Mock body check
        let get_body_z = |e: Entity| -> Option<f32> { if e == body { Some(10.0) } else { None } };

        // Mock hierarchy: grandchild -> child -> body
        let get_parent = |e: Entity| -> Option<Entity> {
            if e == grandchild {
                Some(child)
            } else if e == child {
                Some(body)
            } else {
                None
            }
        };

        // Test finding from self
        let res = find_rigidbody_ancestor(body, get_body_z, get_parent);
        assert_eq!(res, Some((body, 10.0)));

        // Test finding from child
        let res = find_rigidbody_ancestor(child, get_body_z, get_parent);
        assert_eq!(res, Some((body, 10.0)));

        // Test finding from grandchild
        let res = find_rigidbody_ancestor(grandchild, get_body_z, get_parent);
        assert_eq!(res, Some((body, 10.0)));

        // Test orphan (no body found)
        let res = find_rigidbody_ancestor(orphan, get_body_z, get_parent);
        assert_eq!(res, None);
    }
}
