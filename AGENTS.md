# Gradiance - Agent Guidelines

## Project Overview
Gradiance is a Rust/Bevy project recreating Algodoo-style physics.

## Architecture
*   **Core**: `src/lib.rs` defines the `GamePlugin`.
*   **Systems**: `src/systems/` contains specific logic (camera, physics setup).
*   **Tools**: `src/tools/` implements the tool state machine and logic.
    *   Each tool is a plugin that runs systems when in the corresponding `ToolState`.
*   **UI**: `src/ui/` implements the `bevy_egui` toolbar.
*   **Commands**: `src/commands.rs` implements the Undo/Redo stack using a Command Pattern.

## Physics
*   We use `avian2d`.
*   **Units**: 1 World Unit = 1 Pixel (approx). Gravity is scaled or physics scale is adjusted.
*   **Interactions**: Use `SpatialQuery` for picking. Use `Joints` (Distance/Fixed) for dragging to respect physics.

## Testing
*   Use `cargo test`.
*   Unit tests should cover `Command` logic and strictly functional components.
*   Integration tests (running Bevy app) are heavier but possible.

## Code Style
*   Run `cargo fmt` and `cargo clippy` before committing.
*   Prefer functional rust patterns.
*   Use `bevy::prelude::*` for convenience in systems.

## CI
*   GitHub Actions workflow in `.github/workflows/rust.yml`.
