# Issues

## Critical Architecture
- **Deletion bypasses Undo/Redo**: While creation tools (`box_tool`, `circle_tool`) use `GameCommand`, the Delete functionality in `selection.rs` directly despawns entities. This bypasses the Undo system. **Action Required**: Implement `DeleteCommand` and update `selection.rs` to use it.

## Functionality
- **Grid**: Rendering and scaling can be improved. Infinite grid shader not implemented.
- **Ground Plane**: Not infinite. Outline has z-fighting issues when other blocks in contact.
- **Drag Tool**: In pause mode, the mouse joint may accumulate force if the cursor moves, launching objects when unpaused.
- **Inspector**: Tabs should appear on the location of a right click (currently off to the side).
- **Collision**: RevoluteJoint tool (Pin) creates a static body that overlaps with the pinned body. This may cause explosions if collision layers are not managed. Currently, the Pin has no collider (visual only) to avoid this, but it means the pin itself doesn't collide with anything.

## Technical Debt
- **Rendering**: `bevy_prototype_lyon` is currently disabled/commented out due to compatibility issues. Tools currently use simple `Sprite` components as placeholders. **Action Required**: Restore vector rendering when libraries update or find alternative.
- **Polygon Tool**: Uses a complex mix of `tessellate_path` and `ConvexHull` fallback. Robustness could be improved.

## Resolved / Obsolete
- (Resolved) Undo/Redo Architecture: `GameCommand` trait and `CommandStack` implemented. Creation tools converted.
- (Resolved) No move tool (Implemented in Select Tool).
- (Resolved) Highlighting works but only on right click (Select Tool handles this).
- (Resolved) Spacebar should pause/unpause.
- (Resolved) Delete key does not work.
- (Resolved) Physics: `ExternalForce` in Drag Tool replaced with `RevoluteJoint`.
