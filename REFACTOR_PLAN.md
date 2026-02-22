# Refactoring Plan

This document outlines the execution plan for resolving critical technical debt identified in `TECH_DEBT.md`.

## 1. Inspector Undo/Redo Integration (High Priority)
**Problem**: Changes made in the Inspector (via `PropertyChangeEvent`) directly mutate components in `inspector_handlers.rs`, bypassing `CommandStack`. This makes actions irreversible.

**Solution**:
1.  **Create `ModifyPropertyCommand`**:
    - Implement `GameCommand` for property changes.
    - Fields: `entity: EntityId`, `property: PropertyChange`, `old_value: PropertyChange` (for undo).
    - Note: `PropertyChange` enum might need to be refactored to support easy "inverse" or storage of values.
2.  **Refactor `inspector_handlers.rs`**:
    - Instead of mutating components directly, the handler should:
        a. Capture the *current* value of the component (for undo).
        b. Construct a `ModifyPropertyCommand`.
        c. Push it to `CommandStack`.
3.  **Handle Slider Dragging**:
    - **Transient Updates**: While dragging, continue emitting `PropertyChangeEvent` for visual feedback (direct mutation is acceptable for *transient* state if we revert it, but simpler is:).
    - **Commit on Release**: The Inspector UI (`inspector.rs`) should only emit the "Commit" event (or a specific flag) when the mouse button is released.
    - Alternatively, use a "Transient Command" pattern where the command is updated in-place on the stack until committed.
    - **Recommendation**: For V1, allow direct mutation during drag (via existing events), but capture the "Start" value on drag start. On drag end, push a `ModifyPropertyCommand` that represents the diff from Start -> End. This requires the Inspector UI to track "Drag Start" state.

## 2. Fix Pin Collision Instability
**Problem**: `SpawnJointCommand` creates a static "Pin" entity that overlaps with the dynamic body. `SolverGroups` prevents contact resolution, but broadphase collision still occurs, which can cause instability or "explosions" in Rapier.

**Solution**:
1.  **Define Collision Groups**:
    - In `src/prelude.rs` or `physics/mod.rs`, define `GROUP_PIN` (e.g., Group 32) and `GROUP_WORLD` (Group 1).
2.  **Update `SpawnJointCommand` (and friends)**:
    - When spawning the Pin entity:
        ```rust
        Collider::ball(..),
        CollisionGroups::new(GROUP_PIN, Group::ALL), // Membership: PIN, Filter: ALL
        ```
    - When modifying the constrained body (Body A):
        - Retrieve its existing `CollisionGroups`.
        - Update its *filter* to exclude `GROUP_PIN`:
        ```rust
        let mut groups = existing_groups.unwrap_or(CollisionGroups::default());
        groups.filters &= !GROUP_PIN; // Ignore PIN group
        commands.entity(e).insert(groups);
        ```
    - **Undo**: Restore the original `CollisionGroups` in `undo()`.

## 3. Fix Drag Tool Offset
**Problem**: The "Hand" tool snaps the object to the center because it calculates the anchor based on the *current* body transform in the physics step, which might have moved since the input step.

**Solution**:
1.  **Update `DragToolData`**:
    - Store `world_click_pos: Vec2` and `initial_body_transform: Transform` when the drag starts (in `drag_tool_input`).
2.  **Update `drag_tool_physics`**:
    - Use the stored `world_click_pos` to calculate the *initial* local anchor relative to the *initial* body transform.
    - This ensures `local_anchor2` matches exactly where the user clicked on the body.

## 4. Reduce Command Duplication
**Problem**: `SpawnJointCommand`, `SpawnFixedJointCommand`, `SpawnPrismaticJointCommand` share 80% of their code.

**Solution**:
1.  **Create Helper Struct/Functions**:
    - In `src/input/commands.rs`:
    ```rust
    struct JointSpawnHelper {
        entity_a: EntityId,
        entity_b: Option<EntityId>,
        anchor_a: Vec2,
        anchor_b: Vec2,
        // ...
    }

    impl JointSpawnHelper {
        fn resolve_targets(&self, world: &mut World) -> (Entity, Option<Entity>, Vec2, Vec2);
        fn spawn_visual(&self, world: &mut World, shape: impl Shape);
        fn setup_pin_collision(&self, world: &mut World) -> Option<CollisionGroups>;
    }
    ```
2.  **Refactor Commands**:
    - Update each command struct to use these helpers in `apply()` and `undo()`.
