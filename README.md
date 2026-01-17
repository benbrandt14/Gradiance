# Gradiance

**Gradiance** is a modern, open-source 2D physics sandbox inspired by **Algodoo**, built in **Rust** using the [Bevy](https://bevyengine.org/) game engine and [Avian](https://github.com/Jondolf/avian) physics.

## Architecture

Gradiance leverages Bevy's Entity-Component-System (ECS) architecture to create a modular and performant sandbox.

### Core Modules

*   **`src/physics`**: Manages the Avian physics simulation.
    *   Configured for `f64` precision (double precision) to ensure stability in large worlds and complex mechanisms.
    *   High substep count (12-16) for stiff constraints.
    *   Will house custom constraints like Gears and Pulleys.
*   **`src/geometry`**: Handles vector rendering and Constructive Solid Geometry (CSG).
    *   Uses `bevy_prototype_lyon` for vector graphics.
    *   Uses `clipper2` for boolean operations (Cut/Weld) on geometry.
*   **`src/input`**: Manages user interaction and tool states.
    *   Implements a `ToolState` machine (Select, Drag, Cut, Sketch, etc.).
    *   Uses Bevy's built-in picking for object selection.
*   **`src/ui`**: Provides the editor interface.
    *   Built with `bevy_egui` for robust inspector panels and toolbars.
*   **`src/scripting`**: (Planned) Lua integration via `bevy_mod_scripting` for user scripts.

### Physics Configuration

*   **Engine**: Avian 2D (0.5)
*   **Precision**: `f64` (parry-f64)
*   **Integrator**: XPBD (Extended Position-Based Dynamics)

## File Structure

```
src/
├── lib.rs          # GamePlugin definition (Root Plugin)
├── main.rs         # App entry point
├── prelude.rs      # Common imports (Bevy, Avian, Gradiance types)
├── physics/        # Physics configuration & Custom Constraints
├── geometry/       # CSG & Vector Rendering
├── input/          # Tool State & Picking
├── ui/             # Editor UI (Egui)
└── scripting/      # Lua Integration
```

## Getting Started

### Prerequisites
*   [Rust Toolchain](https://rustup.rs/) (Stable)
*   **Linux Dependencies**:
    ```bash
    sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
    ```

### Development

*   **Run**: `cargo run`
*   **Test**: `cargo test` (Note: Full suite may take time)
*   **Check**: `cargo check` (Fast verification)
*   **Lint**: `cargo clippy`
*   **Format**: `cargo fmt`
