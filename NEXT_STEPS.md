# Handover Prompt for Next Agent

You are an expert Rust/Bevy software engineer continuing the refactoring of the 'Gradiance' codebase.
The previous agent has successfully modularized the application (`GradiancePhysicsPlugin`, `GradianceInputPlugin`, etc.) and implemented an Event-Driven Architecture for Tools and the Inspector.

## Current State
-   **Architecture**: Tools emit `SpawnShapeEvent` (and others) instead of queuing commands directly. The Inspector emits `PropertyChangeEvent` instead of mutating components directly.
-   **Tests**: A headless test `tests/tools_headless.rs` verifies basic event-to-command flow.
-   **Known Issue**: The "Alignment" features in the Inspector (`src/ui/inspector.rs`) are currently **disabled** (commented out with TODOs). This is because the original implementation relied on mutable access to components in `InspectorQuery` via closures, which conflicted with the new read-only architecture required for event emission.

## Your Objectives

1.  **Restore Inspector Alignment**:
    -   Refactor `apply_alignment` in `src/ui/inspector.rs`.
    -   Instead of accepting a closure that mutates components, it should accept a closure that *reads* values to calculate the alignment target (Min, Max, Center, Distribute).
    -   Once the target values are calculated, iterate over the selected entities and emit `PropertyChangeEvent`s for each entity to apply the new values.
    -   Uncomment the call sites in `inspector_ui` and verify compilation.

2.  **Expand Headless Tests**:
    -   The current test `tests/tools_headless.rs` only checks `SpawnShapeEvent`.
    -   Add tests for `PropertyChangeEvent` handling (e.g., verifying that sending a `PropertyChange::Transform` event actually updates the entity's transform).

3.  **Refactor Panels (Optional)**:
    -   Review `src/ui/panels.rs`. It currently mutates `Time<Virtual>` and `GridSettings` directly.
    -   If time permits, introduce `TimeScaleEvent` and `GridSettingsEvent` to fully decouple the UI from resources.

## Context
-   See `src/events.rs` for available events.
-   See `src/ui/inspector_handlers.rs` for how property changes are applied.
-   Keep `src/ui/inspector.rs` focused on *reading* state and *emitting* events.
