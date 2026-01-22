# Task: Remove Undo/Redo and Implement Event-Driven Architecture

## Context
The current `CommandStack` (Undo/Redo) implementation is fragile (refer to `ARCHITECTURE_AUDIT.md`) and tightly couples interaction logic with ECS mutation. The decision has been made to **remove the Undo/Redo feature entirely** in favor of a robust, unidirectional **Event-Driven Architecture**.

## Objectives

### 1. Remove Legacy Command Infrastructure
*   **Delete** the `CommandStack` resource and the `GameCommand` trait.
    *   Target: `src/input/commands/mod.rs` (and the logic within).
*   **Remove** the `handle_undo_redo_input` system from `src/input/mod.rs`.
*   **Cleanup** dependencies on `CommandStack` in `src/input/tools/`.

### 2. Define Interaction Events
Create a new module `src/input/events.rs` to define the protocol for modifying the world.
*   **Creation Events:**
    *   `SpawnBoxEvent { position: Vec2, width: f32, height: f32 }`
    *   `SpawnCircleEvent { position: Vec2, radius: f32 }`
    *   `SpawnPolygonEvent { position: Vec2, vertices: Vec<Vec2> }`
    *   `SpawnJointEvent { ... }` (Abstract the various joint types or use an enum).
*   **Mutation Events:**
    *   `ModifyTransformEvent { entity: Entity, translation: Vec3, rotation: Quat }`
    *   `ModifyPhysicsEvent { entity: Entity, body_type: Option<RigidBody>, friction: Option<f32>, ... }`
    *   `ModifyColorEvent { entity: Entity, color: Color }`

### 3. Implement Event Handlers
Create systems that listen to these events and perform the ECS mutations.
*   These systems should generally run in the `Update` schedule, perhaps in a `GameSystemSet::Simulation` set.
*   **Example:** A system reading `EventReader<SpawnBoxEvent>` that runs the logic previously found in `SpawnBoxCommand::apply`.

### 4. Refactor Tools
Update all tools in `src/input/tools/` to **emit events** instead of pushing Commands to the stack.
*   **Box Tool:** On drag release, send `SpawnBoxEvent`.
*   **Drag Tool:** On drag update, send `ModifyTransformEvent`.

### 5. Refactor Inspector UI
Update `src/ui/inspector.rs` to **emit events** instead of directly mutating Query components.
*   The Inspector should read the *current* state from Queries to populate the UI.
*   When a widget is changed, it should fire the appropriate mutation event (e.g., `ModifyPhysicsEvent`).
*   This decouples the UI from the exact details of *how* the change is applied (and ensures systems like "Wake Up Body" are handled centrally in the event handler).

## Guidelines
*   **Simplicity:** Do not over-engineer the event system. Simple Bevy `Event` structs are sufficient.
*   **Verification:** Ensure that clicking and dragging still works, and that the Inspector still updates objects.
*   **Cleanup:** Remove the `src/input/commands/` directory after migrating logic to event handlers.
