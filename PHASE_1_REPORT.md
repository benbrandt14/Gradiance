# Phase 1 Report & Research

## Are the existing systems adequate for modular extension?

**Assessment:** Partially.
The current system uses Bevy Plugins for each tool, which provides a basic level of modularity. New tools can be added without breaking existing ones. However, the integration points are centralized and manual:
1.  `ToolState` enum in `src/input/mod.rs` must be updated.
2.  `PanelsPlugin` in `src/ui/panels.rs` must be updated to add UI buttons.
3.  Each tool re-implements common logic (input handling, UI blocking, grid snapping), leading to code duplication and potential inconsistencies.

## Does the current structure allow this flexibility?

**Assessment:** No, it lacks cohesion.
There is no unified "Tool Framework".
- **Physics vs. Shape**: Logic is mixed inside systems (e.g., `BoxTool` handles both shape creation and physics component spawning).
- **Interaction Modes**: There is no clear separation between "Game Mode" and "Edit Mode" other than simple state checks.
- **Input Handling**: Each tool queries `ButtonInput` and checks `is_pointer_over_ui` individually. This makes it hard to implement global behaviors like "disable all tools" or "intercept input for tutorial".

## Is it possible to add the future items in the roadmap without modifying many files to do so?

**Assessment:** No.
Adding a single new tool currently requires touching:
1.  `src/input/mod.rs` (State)
2.  `src/ui/panels.rs` (UI)
3.  Creating the tool file.
4.  `src/input/tools/mod.rs` (Registration)

While not "many" files, it is not "zero-touch" or configuration-driven.

## Are there other patterns that can be used?

**Recommendation:**
1.  **Command Pattern (`GameCommand`):**
    - **Why:** Essential for Undo/Redo and decoupling Input from Action.
    - **How:** Define a `GameCommand` trait with `apply(&mut World)` and `undo(&mut World)`. Tools simply emit commands.
    - **Benefit:** Allows testing actions in isolation and enables robust Undo/Redo.

2.  **Tool Trait (`Tool`):**
    - **Why:** To standardize input handling and lifecycle.
    - **How:**
      ```rust
      trait Tool {
          fn on_enter(&mut self, world: &mut World);
          fn on_exit(&mut self, world: &mut World);
          fn update(&mut self, world: &mut World, input: &ToolInput);
      }
      ```
    - **Benefit:** Centralizes UI blocking (`is_pointer_over_ui`) and input mapping. The `ToolManager` feeds sanitized input to the active tool.

3.  **Registry Pattern:**
    - Use a `ToolRegistry` resource to dynamically register tools at runtime, avoiding the hardcoded enum and UI switch.

## Are there gaps in testing due to the existing framework itself?

**Assessment:** Yes.
- **UI Coupling:** Systems depend heavily on `EguiContexts`, making them hard to run in headless tests.
- **Input Coupling:** Systems depend on `ButtonInput`, making it hard to simulate complex interaction sequences without a harness.
- **Verification:** There is no easy way to "save" the state of the world to verify tool output (needs Serialization/RON).

**Solution:**
- Abstract input into `ToolInput` events.
- Use `GameCommand` objects which are data-driven and easily testable.
