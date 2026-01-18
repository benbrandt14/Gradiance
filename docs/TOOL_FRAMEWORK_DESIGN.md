# Tool Framework Design Specification

## Overview

The current tool implementation relies on Bevy States (`ToolState`) and ad-hoc systems for each tool. While functional, it lacks a unified lifecycle, making it difficult to implement global features like "Undo/Redo", "Selection Filtering", or "UI Blocking" consistently.

This document outlines the design for a `Tool` trait and a central `ToolManager` to address these issues.

## 1. The `Tool` Trait

Tools will implement a Rust trait rather than just being loose systems. This enforces a consistent API.

```rust
use bevy::prelude::*;

pub trait Tool: Send + Sync {
    /// Unique identifier for the tool (e.g., "box_tool").
    fn id(&self) -> &'static str;

    /// User-facing name (e.g., "Box Tool").
    fn name(&self) -> &'static str;

    /// Icon path or glyph for UI.
    fn icon(&self) -> &'static str;

    /// Lifecycle: Called when the tool becomes active.
    /// Use this to initialize transient state (gizmos, start pos).
    fn on_enter(&mut self, world: &mut World) {}

    /// Lifecycle: Called when the tool is deactivated.
    /// Use this to cleanup.
    fn on_exit(&mut self, world: &mut World) {}

    /// Main update loop.
    /// Returns an optional Command to be executed by the CommandStack.
    /// This decouples Input from Action.
    fn update(&mut self, world: &mut World, input: &ToolInput) -> Option<Box<dyn GameCommand>> {
        None
    }

    /// Draw visual feedback (gizmos).
    /// Separate from logic to allow for "preview" or "ghost" rendering.
    fn draw(&self, gizmos: &mut Gizmos);
}
```

## 2. Input Abstraction (`ToolInput`)

To solve the testing gap, we abstract raw `ButtonInput` and `Window` events into a `ToolInput` struct. This allows us to script inputs for tests.

```rust
pub struct ToolInput {
    pub cursor_pos: Option<Vec2>,
    pub mouse_buttons: ButtonInput<MouseButton>,
    pub keys: ButtonInput<KeyCode>,
    pub is_pointer_over_ui: bool,
    // Add logic for "just_pressed", "drag_delta", etc.
}
```

## 3. Relationship to `GameCommand`

Tools do *not* modify the world directly during `update`. Instead, they return a `GameCommand`.
- **User clicks:** Tool returns `None`.
- **User drags:** Tool updates internal state (ghost box).
- **User releases:** Tool returns `Some(Box::new(SpawnBoxCommand { ... }))`.

This ensures all actions are:
1.  Undoable.
2.  Testable (Tool Output = Command).
3.  Atomic.

## 4. Addressing Specific Constraints

### Selection (Single vs Multi)
The `Tool` trait can access the `Selection` resource via the World.
- **Context Sensitive Tools:** The `ToolManager` can query `tool.is_valid_context(&Selection)` to gray out tools (e.g., "Weld" is only valid if 2 objects are selected).

### Geometry vs Constraints
- **Geometry Tools:** (Box, Circle) Spawn new entities.
- **Constraint Tools:** (Hinge, Spring) Query `SpatialQuery` or `Selection` to find existing bodies and spawn a Joint entity linking them.

### Grid Integration
The `ToolInput` passed to the tool will already be "snapped" if the grid is enabled.
`ToolManager` reads `GridSettings`, snaps the raw cursor position, and populates `ToolInput.cursor_pos` with the snapped value. This removes snapping logic from individual tools.

### UI Interaction
Tools can expose a `ui()` method to render custom settings in the sidebar (e.g., "Number of sides" for Polygon).
```rust
fn ui(&mut self, ui: &mut egui::Ui) {
    ui.add(egui::Slider::new(&mut self.sides, 3..=10).text("Sides"));
}
```

## 5. Plugin Architecture

The system will remain plugin-based but shift registration:

```rust
app.add_plugins(ToolPlugin::<BoxTool>::default());
```

Inside `ToolPlugin`:
```rust
impl<T: Tool + Default + Resource> Plugin for ToolPlugin<T> {
    fn build(&self, app: &mut App) {
        app.init_resource::<T>();
        // Register T with ToolRegistry
    }
}
```

## 6. Migration Strategy

1.  Implement `GameCommand` (Done).
2.  Refactor `BoxTool` to emit `SpawnBoxCommand` (Step 5-6).
3.  Introduce `Tool` trait and `ToolManager`.
4.  Wrap `BoxTool` logic into `impl Tool for BoxTool`.
5.  Replace `ToolState` based switching with `ToolManager` switching.
