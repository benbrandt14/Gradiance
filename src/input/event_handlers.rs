//! Event handlers for converting Game Events into Commands.

use crate::events::*;
use crate::input::commands::{
    CommandStack, DuplicateEntitiesCommand, GameCommand, MoveEntitiesCommand,
    SpawnFixedJointCommand, SpawnGroundCommand, SpawnJointCommand, SpawnPrismaticJointCommand,
    SpawnShapeCommand,
};
use crate::input::selection::Selection;
use crate::prelude::EntityId;
use bevy::prelude::*;

/// Handles `SpawnShapeEvent` by queuing a `SpawnShapeCommand`.
pub fn handle_spawn_shape_event(mut events: EventReader<SpawnShapeEvent>, mut commands: Commands) {
    for event in events.read() {
        let cmd = SpawnShapeCommand {
            position: event.position,
            shape: event.shape.clone(),
            entity: None,
        };
        commands.queue(move |world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.push(Box::new(cmd), world);
            });
        });
    }
}

/// Handles `SpawnGroundEvent` by queuing a `SpawnGroundCommand`.
pub fn handle_spawn_ground_event(
    mut events: EventReader<SpawnGroundEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let cmd = SpawnGroundCommand {
            position: event.position,
            rotation: event.rotation,
            entity: None,
        };
        commands.queue(move |world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.push(Box::new(cmd), world);
            });
        });
    }
}

/// Handles `SpawnJointEvent` by queuing a `SpawnJointCommand`.
pub fn handle_spawn_joint_event(mut events: EventReader<SpawnJointEvent>, mut commands: Commands) {
    for event in events.read() {
        let cmd = SpawnJointCommand {
            entity_a: EntityId(event.entity_a),
            entity_b: event.entity_b.map(EntityId),
            anchor_a: event.anchor_a,
            anchor_b: event.anchor_b,
            compliance: event.compliance,
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

/// Handles `SpawnPrismaticJointEvent` by queuing a `SpawnPrismaticJointCommand`.
pub fn handle_spawn_prismatic_joint_event(
    mut events: EventReader<SpawnPrismaticJointEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let cmd = SpawnPrismaticJointCommand {
            entity_a: EntityId(event.entity_a),
            entity_b: event.entity_b.map(EntityId),
            anchor_a: event.anchor_a,
            anchor_b: event.anchor_b,
            axis: event.axis,
            compliance: event.compliance,
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

/// Handles `SpawnFixedJointEvent` by queuing a `SpawnFixedJointCommand`.
pub fn handle_spawn_fixed_joint_event(
    mut events: EventReader<SpawnFixedJointEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let cmd = SpawnFixedJointCommand {
            entity_a: EntityId(event.entity_a),
            entity_b: event.entity_b.map(EntityId),
            anchor_a: event.anchor_a,
            anchor_b: event.anchor_b,
            compliance: event.compliance,
            rot_a: event.rot_a,
            rot_b: event.rot_b,
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

/// Handles `UndoEvent` by triggering undo on the `CommandStack`.
pub fn handle_undo_event(mut events: EventReader<UndoEvent>, mut commands: Commands) {
    for _ in events.read() {
        commands.queue(|world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.undo(world);
            });
        });
    }
}

/// Handles `RedoEvent` by triggering redo on the `CommandStack`.
pub fn handle_redo_event(mut events: EventReader<RedoEvent>, mut commands: Commands) {
    for _ in events.read() {
        commands.queue(|world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.redo(world);
            });
        });
    }
}

/// Handles `DuplicateEntitiesEvent`.
pub fn handle_duplicate_entities_event(
    mut events: EventReader<DuplicateEntitiesEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let source_ids: Vec<EntityId> = event.entities.iter().map(|e| EntityId(*e)).collect();
        let mut cmd = DuplicateEntitiesCommand {
            source_entities: source_ids,
            created_entities: Vec::new(),
            offset: Vec2::ZERO,
            make_kinematic: event.make_kinematic,
        };

        commands.queue(move |world: &mut World| {
            if cmd.apply(world).is_ok() {
                // Update Selection
                let new_selection: Vec<Entity> = cmd.created_entities.iter().map(|e| e.0).collect();
                if let Some(mut selection) = world.get_resource_mut::<Selection>() {
                    selection.clear();
                    for e in new_selection {
                        selection.add(e);
                    }
                }

                // Push to stack
                world.resource_scope(|_, mut stack: Mut<CommandStack>| {
                    stack.push_without_apply(Box::new(cmd));
                });
            }
        });
    }
}

/// Handles `DragEntitiesEvent` (Transient update).
pub fn handle_drag_entities_event(
    mut events: EventReader<DragEntitiesEvent>,
    mut query: Query<&mut Transform>,
) {
    for event in events.read() {
        for (entity, pos) in &event.positions {
            if let Ok(mut t) = query.get_mut(*entity) {
                t.translation.x = pos.x;
                t.translation.y = pos.y;
            }
        }
        for (entity, rot) in &event.rotations {
            if let Ok(mut t) = query.get_mut(*entity) {
                t.rotation = *rot;
            }
        }
    }
}

/// Handles `CommitDragEvent` (Undoable command).
pub fn handle_commit_drag_event(
    mut events: EventReader<CommitDragEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let pos_changes = event
            .position_changes
            .iter()
            .map(|(e, old, new)| (EntityId(*e), *old, *new))
            .collect();
        let rot_changes = event
            .rotation_changes
            .iter()
            .map(|(e, old, new)| (EntityId(*e), *old, *new))
            .collect();

        let cmd = MoveEntitiesCommand {
            position_changes: pos_changes,
            rotation_changes: rot_changes,
        };

        commands.queue(move |world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.push(Box::new(cmd), world);
            });
        });
    }
}
