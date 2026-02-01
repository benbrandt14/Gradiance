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
- **Description**: The `SpawnJointCommand` creates a "Pin" entity (`RigidBody::Fixed`) overlapping the body. Rapier's solver may violently separate them if collisions aren't disabled.
- **Status**: Logic exists in `SpawnJointCommand` to set `SolverGroups`, but effectiveness needs verification.
- **Action**: Verify if `PIN_GROUP` is correctly ignored by the body's filters.
- **Location**: `src/input/commands.rs`.

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

## Code Quality & Refactoring

### Complex Types & Logic
- **Severity**: Low
- **Description**: Several systems use very complex queries or tuples that should be refactored into `SystemParam` structs or type aliases.
- **Locations**:
    - `src/ui/inspector.rs`: `InspectorQuery`, `extract_inspector_state`.
    - `src/ui/inspector_handlers.rs`.
    - `src/input/tools/drag_tool.rs`.
    - `src/input/tools/select_tool.rs`.

### Argument Count
- **Severity**: Low
- **Description**: Multiple functions exceed the recommended argument count (clippy `too_many_arguments`).
- **Locations**:
    - `src/input/tools/connector.rs`: `update_connector` (12 args).
    - `src/visuals/mod.rs`: `apply_render_settings` (13 args).
    - `src/input/tools/polygon_tool.rs` (8 args).
    - `src/input/tools/utils.rs` (9 args).
    - `src/ui/context_menu.rs` (11 args).
    - `src/ui/menu.rs` (8 args).

### Unused Code
- **Severity**: Low
- **Description**: Unused functions in `inspector.rs`.
- **Locations**: `src/ui/inspector.rs` (`wake_up`, `apply_alignment`).
