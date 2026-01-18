# Issues

## Critical Architecture
- **Undo/Redo Missing**: The project requirements specify an Undo/Redo system via `GameCommand`, but the `GameCommand` trait and `CommandStack` resource are missing from the codebase.
- **Direct Entity Spawning**: Tools (`box_tool`, `circle_tool`, etc.) and `selection.rs` (delete) currently spawn/despawn entities directly using `commands.spawn()`/`commands.despawn()`. This bypasses any future Undo/Redo system. **Action Required**: Implement `GameCommand` trait and refactor all tools to return commands instead of mutating the World directly.

## Functionality
- **Grid**: Rendering and scaling can be improved. Infinite grid shader not implemented.
- **Ground Plane**: Not infinite. Outline has z-fighting issues when other blocks in contact.
- **Drag Tool**: In pause mode, the mouse joint may accumulate force if the cursor moves, launching objects when unpaused.
- **Inspector**: Tabs should appear on the location of a right click (currently off to the side).
- **Collision**: RevoluteJoint tool (Pin) creates a static body that overlaps with the pinned body. This may cause explosions if collision layers are not managed. Currently, the Pin has no collider (visual only) to avoid this, but it means the pin itself doesn't collide with anything.

## Technical Debt
- **Rendering**: `bevy_prototype_lyon` 0.16.0 usage is currently "hacked" in tool implementations (`box_tool`, `circle_tool`, `polygon_tool`, `revolute_joint_tool`). `ShapeBuilder` is used with `build()` to generate a bundle, but standard `Fill` and `Stroke` components are avoided due to component registration/compatibility issues with Bevy 0.18.
- **Polygon Tool**: Uses a complex mix of `tessellate_path` and `ConvexHull` fallback. Robustness could be improved (e.g., decomposing into multiple convex hulls instead of one hull).

## Resolved / Obsolete (To Verify & Archive)
- (Resolved) No move tool (Implemented in Select Tool).
- (Resolved) Highlighting works but only on right click (Select Tool handles this).
- (Resolved) Spacebar should pause/unpause.
- (Resolved) Delete key does not work.
- (Resolved) Physics: `ExternalForce` in Drag Tool replaced with `RevoluteJoint`.
