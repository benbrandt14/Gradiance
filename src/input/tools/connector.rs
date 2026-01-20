//! Unified tool for creating joints (Hinges and Fixes).
//!
//! Handles creation of `RevoluteJoint` and `FixedJoint` with consistent behavior:
//! - Snaps to grid.
//! - Handles single body (pin to world) and two bodies (connect).
//! - Sorts overlapping bodies by Z-depth.
//! - Spawns visual indicators parented to the bodies.

use crate::input::commands::{CommandStack, SpawnFixedJointCommand, SpawnJointCommand};
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
                    matches!(state.get(), ToolState::RevoluteJoint | ToolState::Weld)
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
}

impl ConnectorType {
    fn from_tool_state(state: &ToolState) -> Option<Self> {
        match state {
            ToolState::RevoluteJoint => Some(Self::Hinge),
            ToolState::Weld => Some(Self::Fix),
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
    grid_settings: Res<GridSettings>,
    tool_state: Res<State<ToolState>>,
    bodies: Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: Query<&Parent>,
    transforms: Query<&Transform>,
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
        // Snap to grid
        let pos = if grid_settings.show && grid_settings.snap {
            snap_to_grid(raw_pos, grid_settings.spacing)
        } else {
            raw_pos
        };

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
            if let Ok(t) = transforms.get(e) {
                let rot = t.rotation.to_euler(EulerRot::XYZ).2;
                (calculate_local_anchor(t, pos), rot)
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
                    rot_a,
                    rot_b,
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
}
