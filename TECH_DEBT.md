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
- **Context**: `src/ui/inspector.rs` uses a massive `SystemParam` tuple `InspectorQuery` and `InspectorValue::Mixed` state tracking.
- **Issue**: The query type is extremely large and brittle (`Option<&Transform>`, `Option<&RigidBody>`, etc.). Additionally, the state extraction logic uses a complicated and hard-to-maintain pattern for tracking mixed states across multiple selected entities. This makes it hard to add new components and increases compilation time/complexity. It's a poorly fitting abstraction.
- **Fix**: Refactor `InspectorQuery` into a custom `SystemParam` struct or split the inspector into smaller, focused systems/widgets. Simplify state extraction.
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

### 6. Motor Controller Logic
- **Context**: `MotorController` in `src/physics/controllers.rs` limits and oscillating logic.
- **Issue**: The current implementation calculates joint angles and distances using raw `GlobalTransform` differences, completely ignoring the `local_anchor` values. This is a poorly fitting abstraction that breaks for any joint where anchors aren't at the body origin.
- **Fix**: Re-implement limit detection using Rapier's actual joint properties or properly project local anchors into world space to calculate the true constrained distance/angle.

### 7. Extrusion Material State Loss
- **Context**: 2.5D extrusion in `src/geometry/extrusion.rs`.
- **Issue**: The `create_extruded_mesh` couples mesh generation with material generation, recreating `StandardMaterial` every time the mesh is updated. This discards any custom material modifications made by the user.
- **Fix**: Separate material generation from mesh generation so `Mesh3d` can be updated without overwriting `MeshMaterial3d`.

## Minor / Cleanup

### 8. Code Duplication in Commands
- **Context**: `SpawnJointCommand`, `SpawnFixedJointCommand`, and `SpawnPrismaticJointCommand` share significant logic for resolving targets and spawning visuals.
- **Fix**: Extract common logic into a shared helper or builder pattern within `commands.rs`.

### 9. Unused Dependencies
- **Context**: `Cargo.toml` may contain unused dependencies (e.g. `bevy_vello` is mentioned in docs but is it used?).
- **Fix**: Audit `Cargo.toml` and remove unused crates to improve build times.
