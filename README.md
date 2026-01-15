# Gradiance

**Gradiance** is a modern, open-source 2D physics sandbox inspired by **Algodoo**, built in **Rust** using the [Bevy](https://bevyengine.org/) game engine and [Avian](https://github.com/Jondolf/avian) physics.

## Project Status: Rewrite Phase

We have recently undergone a full rewrite to modernize the codebase and prepare for advanced features.

### Roadmap
- [x] **Core Physics**: Rigidbodies, Colliders, Gravity (Avian2d).
- [x] **Basic Tools**: Box, Circle, Move (Drag/Rotate).
- [x] **Undo/Redo System**: Command pattern implementation.
- [ ] **Advanced Tools**: Polygon, Cutter, Scale.
- [ ] **Mechanisms**: Hinges, Springs, Thrusters, Lasers.
- [ ] **CSG**: Constructive Solid Geometry (Booleans).
- [ ] **Scripting**: User-accessible scripting API.

## Getting Started

### Prerequisites
*   [Rust Toolchain](https://rustup.rs/) (Stable)
*   **Linux Dependencies**:
    ```bash
    sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev clang lld
    ```

### Development

We use a `Makefile` to simplify common tasks:

*   **Run**: `make run` (or `cargo run`)
*   **Test**: `make test` (or `cargo test`)
*   **Lint**: `make lint` (or `cargo clippy`)
*   **Format**: `make fmt` (or `cargo fmt`)

### Controls

*   **Toolbar**: Select tools (Move, Box, Circle) or perform actions (Undo, Redo).
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
*   **Commands**: `GameCommand` trait for reversible actions.
