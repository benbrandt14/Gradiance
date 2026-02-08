# Technical Debt & Architectural Issues

This document tracks architectural hurdles, complex implementations, and known technical debt in the Gradiance codebase.

## High Priority (Critical for Stability)

### 1. Pin Collision Instability
- **Context**: `SpawnJointCommand` and `SpawnFixedJointCommand` create a "Pin" entity (`RigidBody::Fixed`) at the anchor point.
- **Issue**: The pinned body overlaps with this pin. While `SolverGroups` are modified to prevent contact resolution, this is not sufficient for all collision phases (broadphase/narrowphase). Rapier's solver can still react violently if the overlap is not explicitly filtered.
- **Fix**: Implement `CollisionGroups` (membership/filter) to explicitly ignore collisions between the `PIN_GROUP` and the constrained body.
- **Files**: `src/input/commands.rs`

### 2. Drag Tool Offset
- **Context**: The Drag Tool uses a `MouseJoint` (or `RevoluteJoint` equivalent) to move bodies.
- **Issue**: When grabbing a body, the joint anchor should be at the click point relative to the body. Currently, the object may "latch to center" or drift because the local anchor calculation (in input system) and the joint initialization (in physics system) are desynchronized.
- **Fix**: Capture the exact world click position and the body's transform at the moment of the click, and pass this snapshots to the physics system.
- **Files**: `src/input/tools/drag_tool.rs`

## Architectural (Important for Scalability)

### 3. Inspector Query Complexity
- **Context**: `src/ui/inspector.rs` uses a massive `SystemParam` tuple `InspectorQuery`.
- **Issue**: The query type is extremely large and brittle (`Option<&Transform>`, `Option<&RigidBody>`, etc.). This makes it hard to add new components and increases compilation time/complexity.
- **Fix**: Refactor `InspectorQuery` into smaller, focused widgets/systems (e.g., `TransformInspector`, `PhysicsInspector`) or use a custom `SystemParam` struct.
- **Files**: `src/ui/inspector.rs`

### 4. Code Duplication in Joint Commands
- **Context**: `SpawnJointCommand`, `SpawnFixedJointCommand`, and `SpawnPrismaticJointCommand` share significant logic for resolving targets, spawning visuals, and handling pinning.
- **Issue**: Any fix to pinning logic (like Item #1) must be replicated in three places.
- **Fix**: Extract common logic (visual spawning, target resolution, pinning) into a shared helper or builder pattern within `commands.rs`.
- **Files**: `src/input/commands.rs`

## Future Proofing

### 5. Vector Graphics Backend (Lyon vs Vello)
- **Context**: `bevy_prototype_lyon` is used for 2D vector rendering.
- **Issue**: Lyon is CPU-bound for tessellation and may struggle with complex CSG results. `bevy_vello` offers a GPU-accelerated alternative (Compute Shaders) which is better suited for the "Sketch" and "Cut" tools planned for Phase 2.
- **Fix**: Evaluate migrating vector rendering to `bevy_vello` in Phase 2.
- **Files**: `src/geometry/`

### 6. Command Pattern Granularity
- **Context**: `CommandStack` handles Undo/Redo.
- **Issue**: Continuous modifications (like dragging a slider in the inspector) must not flood the undo stack.
- **Fix**: Ensure "Transient" changes (during drag) are applied via events without pushing to history, and only the "Commit" (mouse release) creates a `GameCommand`. (Current Inspector implementation uses `PropertyChangeEvent`, need to verify how these interact with history).

## Cleanup

### 7. Unused Dependencies
- **Context**: `Cargo.toml` needs periodic auditing.
- **Fix**: Remove unused crates (e.g., check `salva2d`, `mimalloc` usage) to improve build times.
