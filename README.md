# Gradiance

Gradiance is a 2D physics sandbox inspired by Algodoo, built in Unity. It features a runtime tool system for creating and interacting with physics objects.

## Getting Started

### Installation
1.  Clone the repository.
2.  Open the project in Unity (Version 2019.4 or later recommended).

### Running the Project
This project uses a code-driven initialization approach. You do **not** need to open a specific scene.
1.  Press the **Play** button in the Unity Editor.
2.  The `SceneInitializer` script will automatically:
    *   Set up the Main Camera.
    *   Initialize the `SimulationManager` and `ToolManager`.
    *   Create the UI (Toolbar).

## Features & Tools

*   **Move Tool**: Select and move objects (Not fully implemented interaction yet).
*   **Box Tool**: Click and drag to create rectangular physics objects.
*   **Circle Tool**: Click and drag to create circular physics objects.
*   **Hinge Tool**: (Placeholder) Create hinge joints.
*   **Spring Tool**: (Placeholder) Create spring joints.
*   **Play/Pause**: Toggle physics simulation.
*   **Clear All**: Remove all physics objects from the scene.

## Project Structure

The codebase has been refactored for clarity and Unity best practices:

*   `Assets/Scripts/Core`: Core managers (`SceneInitializer`, `SimulationManager`).
*   `Assets/Scripts/Physics`: Physics object creation (`PhysicsObjectFactory`).
*   `Assets/Scripts/Tools`: Tool system base classes and implementations (`ToolManager`, `BoxTool`, etc.).
*   `Assets/Scripts/UI`: User Interface management (`UIManager`).
*   `Assets/Scripts/Legacy`: Deprecated scripts from the previous iteration.

## Controls
*   **Left Click + Drag**: Use the selected tool (e.g., draw a box).
*   **UI Toolbar**: Select tools or control simulation at the top of the screen.

