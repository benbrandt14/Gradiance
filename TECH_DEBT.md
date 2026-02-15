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
- **Issue**: When grabbing a body, the joint anchor should be at the click point relative to the body. Currently, the object may "latch to center" or drift because the local anchor calculation or the joint initialization doesn't perfectly match the world click position at the start of the drag. The `local_anchor` is calculated based on the transform at input time, but applied in physics time.
- **Fix**: Capture exact world click position and body transform in `DragToolData` at the moment of the click, and use them to calculate the precise local anchor.
- **Files**: `src/input/tools/drag_tool.rs`

### 3. Code Duplication in Commands
- **Context**: `SpawnJointCommand`, `SpawnPrismaticJointCommand`, and `SpawnFixedJointCommand` share significant logic for resolving targets, handling pins, and spawning visuals.
- **Issue**: This duplication makes it harder to maintain joint logic (e.g. fixing the Pin Collision issue requires changes in 3 places).
- **Fix**: Extract common logic into a shared helper or builder pattern within `commands.rs` or a separate module.
- **Files**: `src/input/commands.rs`

### 4. Inspector Query Complexity
- **Context**: `src/ui/inspector.rs` uses a massive `SystemParam` tuple `InspectorQuery`.
- **Issue**: The query type is extremely large and brittle (`Option<&Transform>`, `Option<&RigidBody>`, etc.). This makes it hard to add new components and increases compilation time/complexity. `extract_inspector_state` is also a monolithic function.
- **Fix**: Refactor `InspectorQuery` into a custom `SystemParam` struct or split the inspector into smaller, focused systems/widgets. Use `bevy_inspector_egui` patterns where appropriate.
- **Files**: `src/ui/inspector.rs`

## Medium Priority

### 5. Motor Controller Logic
- **Context**: `src/physics/controllers.rs` implements a simple motor oscillation controller.
- **Issue**: The logic uses global transform differences (`rot_b - rot_a`) to estimate joint angles. This ignores the joint's local anchor frames (`local_axis`, `local_basis`). If the joint was created with non-aligned frames, this calculation is incorrect.
- **Fix**: Query Rapier's internal joint state for the true position/angle, or correctly implement the transform math involving local anchors.
- **Files**: `src/physics/controllers.rs`

### 6. Vector Graphics Backend (Lyon vs Vello)
- **Context**: `bevy_prototype_lyon` is used for 2D vector rendering.
- **Issue**: Lyon is CPU-bound for tessellation and may struggle with complex CSG results. `bevy_vello` offers a GPU-accelerated alternative (Compute Shaders) which is better suited for the "Sketch" and "Cut" tools planned for Phase 2.
- **Fix**: Evaluate migrating vector rendering to `bevy_vello`.
- **Files**: `src/geometry/`

## Low Priority

### 7. Infinite Floor Rendering
- **Context**: The "Infinite Floor" is a large `Collider::cuboid`.
- **Issue**: It lacks a visual grid that scales with the camera, making it hard to judge scale and movement.
- **Fix**: Implement a custom shader material for the ground plane that renders a grid in world space.
- **Files**: `src/physics/floor.rs`

### 8. Command Pattern Granularity
- **Context**: `CommandStack` handles Undo/Redo.
- **Issue**: Continuous modifications (like dragging a slider in the inspector) are not batched. This could technically flood the undo stack, although the Inspector currently emits events that might be handled differently. If every frame of a drag creates a command, it's a problem.
- **Fix**: Ensure "Transient" changes (during drag) are applied directly or via temporary events, and only the "Commit" (mouse release) creates a `GameCommand`.
- **Files**: `src/input/commands.rs`

### 9. Unused Dependencies
- **Context**: `Cargo.toml` may contain unused dependencies.
- **Fix**: Audit `Cargo.toml` and remove unused crates.
