# Technical Debt & Architectural Issues

This document tracks architectural hurdles, complex implementations, and known technical debt in the Gradiance codebase.

## High Priority

### 1. Pin Collision Instability
- **Context**: `SpawnJointCommand` and `SpawnFixedJointCommand` create a "Pin" entity (`RigidBody::Fixed`) at the anchor point.
- **Issue**: The pinned body overlaps with this pin. While `SolverGroups` are modified to prevent contact resolution, this may not sufficient for all collision phases or third-party plugins. Rapier's solver can still react violently if the overlap is not explicitly filtered at the broadphase level.
- **Fix**: Implement `CollisionGroups` (membership/filter) to explicitly ignore collisions between the `PIN_GROUP` and the constrained body.
- **Files**: `src/input/commands.rs`

### 2. Drag Tool Offset
- **Context**: The Drag Tool uses a `MouseJoint` (or `RevoluteJoint` equivalent) to move bodies.
- **Issue**: When grabbing a body, the joint anchor should be at the click point relative to the body. Currently, the object may "latch to center" or drift because the local anchor calculation or the joint initialization doesn't perfectly match the world click position at the start of the drag.
- **Fix**: Verify `calculate_local_anchor` and ensure `local_anchor1` (hand) and `local_anchor2` (body) preserve the initial relative transform.
- **Files**: `src/input/tools/drag_tool.rs`

## Architectural

### 3. Inspector Query Complexity
- **Context**: `src/ui/inspector.rs` uses a massive `SystemParam` tuple `InspectorQuery`.
- **Issue**: The query type is extremely large and brittle (`Option<&Transform>`, `Option<&RigidBody>`, etc.). This makes it hard to add new components and increases compilation time/complexity.
- **Fix**: Refactor `InspectorQuery` into a custom `SystemParam` struct or split the inspector into smaller, focused systems/widgets.
- **Files**: `src/ui/inspector.rs`

### 4. Vector Graphics Backend (Lyon vs Vello)
- **Context**: `bevy_prototype_lyon` is used for 2D vector rendering.
- **Issue**: Lyon is CPU-bound for tessellation and may struggle with complex CSG results. `bevy_vello` offers a GPU-accelerated alternative (Compute Shaders) which is better suited for the "Sketch" and "Cut" tools planned for Phase 2.
- **Fix**: Evaluate migrating vector rendering to `bevy_vello`.
- **Files**: `src/geometry/`

### 5. Command Pattern Granularity
- **Context**: `CommandStack` handles Undo/Redo.
- **Issue**: Continuous modifications (like dragging a slider in the inspector) are not batched. This could technically flood the undo stack, although the Inspector currently emits events that might be handled differently. If every frame of a drag creates a command, it's a problem.
- **Fix**: Ensure "Transient" changes (during drag) are applied directly or via temporary events, and only the "Commit" (mouse release) creates a `GameCommand`.

## Minor / Cleanup

### 6. Code Duplication in Commands
- **Context**: `SpawnJointCommand`, `SpawnFixedJointCommand`, and `SpawnPrismaticJointCommand` share significant logic for resolving targets and spawning visuals.
- **Fix**: Extract common logic into a shared helper or builder pattern within `commands.rs`.
