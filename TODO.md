# Future Work / TODO

## High Priority
- [ ] **Move Tool Implementation**: Implement `MoveTool` logic to select rigidbodies under the cursor and move them using `TargetJoint2D` or Transform manipulation.
- [ ] **Hinge & Spring Tools**: Implement logic for `HingeTool` and `SpringTool` to connect two rigidbodies or anchor them to the background.
- [ ] **Object Selection**: Add visual feedback for selected objects (outline or bounding box).
- [ ] **Context Menu**: Implement `ContextMenuController` to allow right-clicking objects to change properties (color, material, layer, etc.).

## Improvements
- [ ] **Serialization**: Implement a Save/Load system to persist scenes (JSON or XML).
- [ ] **Undo/Redo System**: Command pattern implementation for actions.
- [ ] **UI Polish**: Replace runtime-generated UI with a proper Prefab-based UI or UI Toolkit for better layout and styling.
- [ ] **Camera Controls**: Add pan and zoom functionality to the Main Camera.

## Code Debt
- [x] **Test Coverage**: Added unit tests for `PhysicsObjectFactory`, `ToolManager`, and `SimulationManager`.
- [ ] **Physics Materials**: Add support for friction and bounciness configuration.
- [ ] **Collision Layers**: Set up collision layers/masks to prevent objects from colliding with themselves if complex shapes are added.
