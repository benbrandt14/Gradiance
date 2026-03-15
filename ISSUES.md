# Issues

## Functionality
- **Grid**: Rendering and scaling can be improved. Infinite grid shader not implemented, not currently visible.
- **Ground Plane**: Not infinite. Does not have a tool to create it.
- **Drag Tool**: In play mode the object is not interacted upon with the correct offset, it latches to the center incorrectly
- **Rotate**: Right click + drag should rotate items
- **Scale**: Not implemented, when shape is selected and scale tool is active, scale handles should appear on the bounding box 
- **Inspector**: Minimal context menu items & actions available.
- **Collision**: RevoluteJoint tool (Pin) creates a static body that overlaps with the pinned body. This may cause explosions if collision layers are not managed. Currently, the Pin has no collider (visual only) to avoid this, but it means the pin itself doesn't collide with anything.

# Technical Debt
- **Polygon Tool**: Does not always create correct nonconvex collider.

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

### Inspector Query Complexity
- **Severity**: Medium
- **Description**: `InspectorQuery` in `src/ui/inspector.rs` is a brittle, large tuple struct that should be split or refactored.

### Code Duplication in Commands
- **Severity**: Low
- **Description**: Code duplication in `src/input/commands.rs` for `SpawnJointCommand`, `SpawnFixedJointCommand`, and `SpawnPrismaticJointCommand` for visual spawning and target resolution that must be extracted to a shared helper.

### Excessive Arguments in Tools
- **Severity**: Low
- **Description**: `update_connector` in `src/input/tools/connector.rs` has excessive arguments and needs refactoring.
