# Issues

## Functionality
- **Grid**: Rendering and scaling can be improved. Infinite grid shader not implemented, not currently visible.
- **Ground Plane**: Not infinite. Does not have a tool to create it.
- **Drag Tool**: In play mode the object is not interacted upon with the correct offset, it latches to the center incorrectly
- **Rotate**: Right click + drag should rotate items
- **Scale**: Not implemented, when shape is selected and scale tool is active, scale handles should appear on the bounding box 
- **Inspector**: Minimal context menu items & actions available.
- **Collision**: RevoluteJoint tool (Pin) creates a static body that overlaps with the pinned body. This may cause explosions if collision layers are not managed. Currently, the Pin has no collider (visual only) to avoid this, but it means the pin itself doesn't collide with anything.

## Technical Debt
- **Rendering**: Vector rendering has been buggy, just use line gizmos.
- **Polygon Tool**: Could be improved, does not always create correct nonconvex collider.
