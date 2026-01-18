//! Unified tool for creating joints (Hinges and Fixes).
//!
//! Handles creation of `RevoluteJoint` and `FixedJoint` with consistent behavior:
//! - Snaps to grid.
//! - Handles single body (pin to world) and two bodies (connect).
//! - Sorts overlapping bodies by Z-depth.
//! - Spawns visual indicators parented to the bodies.

use crate::input::tools::utils::{calculate_local_anchor, is_pointer_over_ui};
use crate::input::{ToolState, cursor::CursorWorldPos};
use crate::prelude::*;
use crate::ui::grid::{GridSettings, snap_to_grid};
use avian2d::prelude::*;
use bevy::ecs::relationship::Relationship;
use bevy::math::DVec2;
use bevy_egui::EguiContexts;
use bevy_prototype_lyon::prelude::*;
use std::cmp::Ordering;

/// Plugin for the Connector Tool.
pub struct ConnectorToolPlugin;

impl Plugin for ConnectorToolPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update_connector.run_if(|state: Res<State<ToolState>>| {
                matches!(state.get(), ToolState::RevoluteJoint | ToolState::Weld)
            }),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectorType {
    Hinge,
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
    spatial_query: SpatialQuery,
    mut contexts: EguiContexts,
    grid_settings: Res<GridSettings>,
    tool_state: Res<State<ToolState>>,
    bodies: Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: Query<&ChildOf>,
    transforms: Query<&Transform>,
    global_transforms: Query<&GlobalTransform>,
    rigid_bodies: Query<&RigidBody>,
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
        let shape = Collider::circle(0.1);
        let filter = SpatialQueryFilter::default();
        let intersections = spatial_query.shape_intersections(&shape, pos, 0.0, &filter);

        // Resolve to bodies and sort
        let sorted_bodies = resolve_sorted_bodies(&intersections, &bodies, &parents);

        if sorted_bodies.is_empty() {
            return;
        }

        if sorted_bodies.len() == 1 {
            // Pin single body to world
            create_joint(
                &mut commands,
                &transforms,
                &global_transforms,
                &rigid_bodies,
                connector_type,
                sorted_bodies[0],
                None,
                pos,
            );
        } else {
            // Connect top two bodies
            create_joint(
                &mut commands,
                &transforms,
                &global_transforms,
                &rigid_bodies,
                connector_type,
                sorted_bodies[0],       // Top
                Some(sorted_bodies[1]), // Bottom
                pos,
            );
        }
    }
}

/// Resolves a list of intersected entities (likely colliders) to their root RigidBody ancestors,
/// deduplicates them, and sorts them by Z-index (Descending: Top first).
fn resolve_sorted_bodies(
    intersections: &[Entity],
    bodies: &Query<(Entity, &GlobalTransform), With<RigidBody>>,
    parents: &Query<&ChildOf>,
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
    resolved.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

    resolved.into_iter().map(|(e, _)| e).collect()
}

fn create_joint(
    commands: &mut Commands,
    transforms: &Query<&Transform>,
    global_transforms: &Query<&GlobalTransform>,
    rigid_bodies: &Query<&RigidBody>,
    connector_type: ConnectorType,
    entity_a: Entity,
    entity_b: Option<Entity>,
    anchor_world: DVec2,
) {
    // If Fix (2 Bodies), we use parenting strategy
    if connector_type == ConnectorType::Fix && entity_b.is_some() {
        let b = entity_b.unwrap();
        let rb_a = rigid_bodies.get(entity_a).ok();
        let rb_b = rigid_bodies.get(b).ok();

        // Determine hierarchy: Parent should be Static if possible, else Bottom (B).
        // A is Top, B is Bottom.
        let (parent, child) = if let Some(RigidBody::Static) = rb_b {
            (b, entity_a)
        } else if let Some(RigidBody::Static) = rb_a {
            (entity_a, b)
        } else {
            (b, entity_a) // Attach Top to Bottom
        };

        if let (Ok(p_global), Ok(c_global)) =
            (global_transforms.get(parent), global_transforms.get(child))
        {
            // Re-parenting logic
            // We use set_parent_in_place to preserve GlobalTransform.
            // We also strip RigidBody and Velocity components to merge the physics body.

            commands
                .entity(child)
                .remove::<RigidBody>()
                .remove::<LinearVelocity>()
                .remove::<AngularVelocity>()
                .set_parent_in_place(parent);

            // Note: set_parent_in_place automatically calculates the correct local Transform.
            // We do not need to manually insert it.

            // Spawn Visual on Parent at anchor
            let visual_anchor_local = calculate_local_anchor_from_global(p_global, anchor_world);

            spawn_visual(commands, parent, visual_anchor_local, connector_type);
        }
        return;
    }

    // Standard Joint Creation (Hinge or Fix-1-Body)

    let get_local_and_rot = |e: Entity| -> (DVec2, f64) {
        if let Ok(t) = transforms.get(e) {
            let rot = t.rotation.to_euler(EulerRot::XYZ).2 as f64;
            (calculate_local_anchor(t, anchor_world), rot)
        } else {
            (DVec2::ZERO, 0.0)
        }
    };

    let (anchor_a, rot_a) = get_local_and_rot(entity_a);

    // Spawn Visual on Body A
    let visual_entity = spawn_visual(commands, entity_a, anchor_a, connector_type);

    if let Some(entity_b) = entity_b {
        let (anchor_b, rot_b) = get_local_and_rot(entity_b);
        // Hinge (2 Bodies)
        match connector_type {
            ConnectorType::Hinge => {
                commands.entity(visual_entity).insert(
                    RevoluteJoint::new(entity_a, entity_b)
                        .with_local_anchor1(anchor_a)
                        .with_local_anchor2(anchor_b)
                        .with_point_compliance(0.0),
                );
            }
            ConnectorType::Fix => {
                // Should not happen here due to early return, but kept for safety/fallback
                commands.entity(visual_entity).insert(
                    FixedJoint::new(entity_a, entity_b)
                        .with_local_anchor1(anchor_a)
                        .with_local_anchor2(anchor_b)
                        .with_local_basis2(Rot2::radians((rot_a - rot_b) as f32)),
                );
            }
        }
    } else {
        // Connect to World (Pin)
        let pin = commands
            .spawn((
                RigidBody::Static,
                Transform::from_xyz(anchor_world.x as f32, anchor_world.y as f32, 0.0),
            ))
            .id();

        let anchor_b = DVec2::ZERO;

        match connector_type {
            ConnectorType::Hinge => {
                commands.entity(visual_entity).insert(
                    RevoluteJoint::new(entity_a, pin)
                        .with_local_anchor1(anchor_a)
                        .with_local_anchor2(anchor_b)
                        .with_point_compliance(0.0),
                );
            }
            ConnectorType::Fix => {
                commands.entity(visual_entity).insert(
                    FixedJoint::new(entity_a, pin)
                        .with_local_anchor1(anchor_a)
                        .with_local_anchor2(anchor_b)
                        .with_local_basis2(Rot2::radians(rot_a as f32)),
                );
            }
        }
    }
}

fn calculate_local_anchor_from_global(
    global_transform: &GlobalTransform,
    world_point: DVec2,
) -> DVec2 {
    let transform = global_transform.compute_transform();
    calculate_local_anchor(&transform, world_point)
}

fn spawn_visual(
    commands: &mut Commands,
    parent: Entity,
    local_pos: DVec2,
    connector_type: ConnectorType,
) -> Entity {
    let visual_entity = commands
        .spawn((
            Transform::from_xyz(local_pos.x as f32, local_pos.y as f32, 0.1),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            Collider::circle(0.5),
            Sensor,
        ))
        .set_parent_in_place(parent)
        .id();

    match connector_type {
        ConnectorType::Hinge => {
            commands
                .entity(visual_entity)
                .insert((ShapeBuilder::with(&shapes::Circle {
                    radius: 5.0,
                    ..default()
                })
                .fill(Color::BLACK)
                .build(),));
            let inner = commands
                .spawn((
                    ShapeBuilder::with(&shapes::Circle {
                        radius: 2.0,
                        ..default()
                    })
                    .fill(Color::WHITE)
                    .build(),
                    Transform::from_translation(Vec3::Z * 0.1),
                ))
                .id();
            commands.entity(visual_entity).add_child(inner);
        }
        ConnectorType::Fix => {
            let v1 = commands
                .spawn((
                    ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, -3.0), Vec2::new(3.0, 3.0)))
                        .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                        .build(),
                    Transform::from_translation(Vec3::Z * 0.1),
                ))
                .id();
            let v2 = commands
                .spawn((
                    ShapeBuilder::with(&shapes::Line(Vec2::new(-3.0, 3.0), Vec2::new(3.0, -3.0)))
                        .stroke(Stroke::new(Color::srgb(1.0, 0.0, 0.0), 1.0))
                        .build(),
                    Transform::from_translation(Vec3::Z * 0.1),
                ))
                .id();
            commands.entity(visual_entity).add_children(&[v1, v2]);
        }
    }
    visual_entity
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
}
