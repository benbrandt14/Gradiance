# Gradiance

Gradiance is a 2D physics sandbox inspired by Algodoo, built in Rust using the [Bevy](https://bevyengine.org/) game engine and [Avian](https://github.com/Jondolf/avian) physics.

## Getting Started

### Prerequisites
*   [Rust Toolchain](https://rustup.rs/) (Stable)
*   System dependencies (Linux):
    ```bash
    sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev
    ```

### Running
```bash
cargo run
```

### Controls
*   **Toolbar**: Select tools (Move, Box, Circle) or perform actions (Undo, Redo, Clear).
*   **Pan**: Middle Mouse Drag.
*   **Zoom**: Scroll Wheel.
*   **Move Tool**:
    *   **Drag**: Left Click and Drag to move objects physically.
    *   **Rotate**: Hold `Shift` + Left Click to rotate objects.
*   **Box/Circle Tool**: Left Click and Drag to create shapes.

## Architecture
*   **ECS**: Bevy Entity Component System.
*   **Physics**: Avian2d (formerly `bevy_xpbd`).
*   **UI**: `bevy_egui` (Immediate Mode GUI).
*   **Tools**: State-based tool system (`ToolState`).

## Development
*   **Test**: `cargo test`
*   **Lint**: `cargo clippy`
