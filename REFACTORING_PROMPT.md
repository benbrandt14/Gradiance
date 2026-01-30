You are an expert Rust/Bevy software engineer. Your task is to refactor the 'Gradiance' codebase to strictly adhere to Bevy best practices, specifically focusing on modularity, testability, and clean architecture.

## Objectives

1.  **Plugin Modularization**:
    -   Decompose the monolithic `GamePlugin` and its sub-modules (`input`, `ui`, `physics`, `visuals`) into independent, reusable crates or self-contained plugins.
    -   **Physics**: Create a `GradiancePhysicsPlugin` that handles all rapier config, constraints, and the floor.
    -   **Input**: Create a `GradianceInputPlugin` that manages tools, cursor, and selection. Isolate `bevy_egui` dependencies so input logic can be tested without UI.
    -   **Visuals**: Create a `GradianceVisualsPlugin` for rendering, effects (bloom, toon shader), and gizmos.
    -   **UI**: Create a `GradianceUiPlugin` that *only* handles Egui rendering and widgets. It must not mutate game state directly.

2.  **Event-Driven Architecture**:
    -   Replace all direct resource mutation in UI/Input systems with `EventWriter` triggers.
    -   **Command Pattern**: Introduce a robust event bus for game actions (e.g., `SpawnShapeEvent`, `SelectEntityEvent`, `ChangeToolEvent`).
    -   **System**: Create dedicated systems that read these events and apply changes to the `World` (ECS) or `CommandStack`.

3.  **UI/Logic Separation**:
    -   Refactor all tools in `src/input/tools/*.rs` to be pure logic systems.
    -   Pass UI state (like "is pointer over UI") as a resource or event, never as a direct dependency on `EguiContexts` within logic systems.
    -   Refactor `Inspector` and `Panels` (UI) to only *read* component data and *emit* change events.

4.  **Entity Management**:
    -   Implement `StrongID` wrappers (e.g., `struct EntityId(Entity)`) for persistent references.
    -   Ensure all spawned entities have a `Name` component for debugging.
    -   Use `StateScoped` or a custom cleanup component to manage entity lifecycles per GameState.

5.  **Testing**:
    -   Ensure all refactored logic is covered by unit tests in `tests/`.
    -   Because logic is now decoupled from Egui/Winit, write headless tests that verify:
        -   Events trigger correct state changes.
        -   Tools produce correct commands.
        -   Undo/Redo stack integrity.

## Constraints
-   Do not break existing functionality.
-   Maintain the current `TODO` comments as a checklist until addressed.
-   Keep the codebase compiling at every step.

Begin by analyzing the `TODO` comments added in the previous audit to guide your specific changes.
