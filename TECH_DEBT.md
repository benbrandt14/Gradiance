# Technical Debt & Architectural Issues

This document tracks known technical debt, architectural challenges, and "half-baked" patterns in the Gradiance codebase.

## Physics & Constraints

### Pin Collision Instability
- **Severity**: High
- **Description**: The `SpawnJointCommand` (for Hinges and Fixed joints) creates a "Pin" entity (`RigidBody::Fixed`) when connecting a dynamic body to the world. Currently, this pin is spawned at the anchor point. If the dynamic body overlaps with this pin (which it must to be pinned), Rapier's solver may attempt to separate them violently if collision is not disabled.
- **Fix**: Implement `CollisionGroups` or collision filtering in `SpawnJointCommand` to ignore collisions between the pin entity and the constrained body.
- **Location**: `src/input/commands.rs`, `SpawnJointCommand::apply`.

### Joint Limits & Breakage
- **Severity**: Medium
- **Description**: Joints currently have no limits (angle limits for hinges, force limits for breakage). This allows for unrealistic behavior (infinite spinning, unbreakability).
- **Fix**: Expose `ImpulseJoint` limits in the UI and command structure.

## Rendering & Visualization

### Vector Graphics Backend
- **Severity**: Medium
- **Description**: The project relies on `bevy_prototype_lyon` (0.13.0) for vector shape rendering. While functional, it has performance limitations with complex shapes and compatibility issues with future Bevy versions are a concern.
- **Alternative**: `bevy_vello` is present in dependencies but unused. Vello offers high-performance GPU vector rendering.
- **Goal**: Evaluate migrating from Lyon to Vello for the "Sketch" and "CSG" phases.

### Infinite Floor Rendering
- **Severity**: Low (Visual)
- **Description**: The "Infinite Floor" is currently implemented as a very large `Collider::cuboid` (100k units). While physically functional, it lacks a true infinite grid shader, making navigation at large scales disorienting.
- **Fix**: Implement a custom shader material for the ground plane that renders a grid in world space, independent of the mesh UVs.

## Architecture

### Command Pattern Scope
- **Severity**: Low
- **Description**: The `CommandStack` pattern works well for creation/deletion. However, modifying continuous properties (like dragging a slider for stiffness) via individual commands can flood the undo stack.
- **Refactoring**: Consider a "Transaction" or "Transient Command" pattern for continuous modifications.

### Input Handling
- **Severity**: Low
- **Description**: `DragTool` has known offset issues where the object snaps to the center rather than maintaining the grab offset.
- **Location**: `src/input/tools/drag_tool.rs`.
