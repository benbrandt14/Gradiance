# Issues

- Grid rendering and scaling can be improved.
- Ground plane is not infinite, outline has z-fighting issues when other blocks in contact.
- No move tool (or functionality).
- Highlighting works but only on right click, not left click or box select - should be any selection.
- Box tool should also be a select tool (Box Selection).
- Spacebar should pause/unpause.
- Delete key does not work.
- Delete via UI causes a warning about invalid entity generation.
- Drag tool in pause mode does nothing (good), then launches objects when unpaused (bad)
- Inspector tabs should appear on the location of a right click (currently off to the side).
- **Technical Debt:** `bevy_prototype_lyon` 0.16.0 usage is currently "hacked" in tool implementations (`box_tool`, `circle_tool`, `polygon_tool`, `revolute_joint_tool`). `ShapeBuilder` is used with `build()` but standard `Fill` and `Stroke` components are avoided/worked around due to component registration issues with Bevy 0.18.
- **Physics:** `ExternalForce` in Drag Tool uses simple PD controller which might be unstable at high forces/dampings.
