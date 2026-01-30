//! Event handlers for converting Game Events into Commands.

use bevy::prelude::*;
use crate::events::*;
use crate::input::commands::{
    CommandStack, SpawnShapeCommand, SpawnGroundCommand, SpawnJointCommand,
    SpawnPrismaticJointCommand, SpawnFixedJointCommand
};

/// Handles `SpawnShapeEvent` by queuing a `SpawnShapeCommand`.
pub fn handle_spawn_shape_event(
    mut events: EventReader<SpawnShapeEvent>,
    mut commands: Commands,
) {
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
pub fn handle_spawn_joint_event(
    mut events: EventReader<SpawnJointEvent>,
    mut commands: Commands,
) {
    for event in events.read() {
        let cmd = SpawnJointCommand {
            entity_a: event.entity_a,
            entity_b: event.entity_b,
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
            entity_a: event.entity_a,
            entity_b: event.entity_b,
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
            entity_a: event.entity_a,
            entity_b: event.entity_b,
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
pub fn handle_undo_event(
    mut events: EventReader<UndoEvent>,
    mut commands: Commands,
) {
    for _ in events.read() {
        commands.queue(|world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.undo(world);
            });
        });
    }
}

/// Handles `RedoEvent` by triggering redo on the `CommandStack`.
pub fn handle_redo_event(
    mut events: EventReader<RedoEvent>,
    mut commands: Commands,
) {
    for _ in events.read() {
        commands.queue(|world: &mut World| {
            world.resource_scope(|world, mut stack: Mut<CommandStack>| {
                stack.redo(world);
            });
        });
    }
}
