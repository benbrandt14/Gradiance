# Instructions for Agents

This project, **Gradiance**, is a modern rewrite of the classic physics sandbox **Algodoo**, built with **Rust** and the **Bevy** game engine.

## Core Directives

1.  **Modern Algodoo Rewrite**: The ultimate goal is to replicate and improve upon Algodoo's features (lasers, thrusters, CSG, scripting) in a modern engine.
2.  **Test-Driven Development (TDD)**:
    *   **Tests First**: Whenever possible, write unit tests for logic before implementing the feature.
    *   **Massive Coverage**: Aim for high test coverage, especially for `GameCommand` implementations and tool logic.
    *   Use `rstest` for expressive testing fixtures.
3.  **Code Style**:
    *   Prefer **functional programming** patterns where applicable (e.g., iterators, `map`, `filter`) over imperative loops.
    *   Keep systems small and focused.
    *   Use `clippy` to ensure idiomatic Rust.
4.  **Architecture**:
    *   **ECS**: Strictly adhere to Bevy's Entity Component System.
    *   **Tools**: Implement tools as separate plugins managed by `ToolState`.
    *   **Commands**: All gameplay actions (creation, deletion, modification) MUST implement `GameCommand` to support Undo/Redo via `CommandStack`.

## Future Roadmap (Design Goals)

*   **Scripting**: A scripting layer (likely Rhai or Lua) to allow users to control objects programmatically.
*   **CSG**: Constructive Solid Geometry for cutting and merging shapes.
*   **Lasers & Optics**: Raycasting and reflection/refraction systems.
*   **Thrusters**: Physics-based propulsion components.
*   **Material Editor**: Custom friction, restitution, and density per object.

## Development Environment

*   Use `cargo test` to run tests.
*   Use `cargo clippy` for linting.
*   Use the provided `Makefile` for common tasks (`make run`, `make test`, `make fmt`).
