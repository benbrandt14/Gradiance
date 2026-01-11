# Gradiance - Agent Guidelines

## Project Overview
Gradiance is a Unity project aiming to recreate Algodoo-style interactive 2D physics. The goal is to build a robust, user-friendly physics sandbox with tools for creating, manipulating, and simulating 2D objects.

## Development Status
*   **Core**: Basic `SimulationManager` and `SceneInitializer` exist.
*   **Physics**: `PhysicsObjectFactory` handles creation of Boxes and Circles.
*   **Tools**: Basic `ToolManager` and tool stubs (`BoxTool`, `CircleTool`, `MoveTool`, `HingeTool`, `SpringTool`) exist.
*   **UI**: `UIManager` and basic Context Menu controller exist.

## Architectural Roadmap
### Phase 1: Core Functionality (Immediate)
*   **Move Tool**: Implement logic to select rigidbodies, drag them, and rotate them.
*   **Joints**: Implement `HingeTool` and `SpringTool` to connect objects.
*   **Selection**: Visual feedback for selected objects (outlines/bounding boxes).
*   **Context Menu**: Functional right-click menu for object properties.

### Phase 2: Robustness & Data
*   **Undo/Redo**: Implement the Command Pattern for all actions (creation, deletion, modification).
*   **Serialization**: Save and Load scenes to/from JSON/XML.
*   **Scene Management**: Better handling of clearing and reloading scenes.

### Phase 3: Polish & Advanced Features
*   **UI Inspector**: A property inspector panel for fine-tuning object values.
*   **Camera Controls**: Pan and Zoom functionality.
*   **CSG/Geometry**: Polygon tool and boolean operations.
*   **Layers & Collision**: Configurable collision matrices.

## Environment & Testing
*   **Testing**: Since this is a Unity project, standard NUnit tests are used. For CI/CD in headless environments (like this agent VM), we use a **Mock UnityEngine** approach.
    *   **Run Tests**: `dotnet test Tests/Gradiance.UnitTests/Gradiance.UnitTests.csproj`
    *   **Setup**: Run `bash setup.sh` to install the .NET SDK.
    *   **Coverage**:
        *   `PhysicsObjectFactoryTests`: Verifies object creation (Box, Circle) and component initialization.
        *   `ToolManagerTests`: Verifies tool registration, selection, and callback execution.
        *   `SimulationManagerTests`: Verifies singleton, pause/resume logic, and global settings like gravity.
*   **Compilation**: The `Verification.cs` script in `Assets/Scripts` is a compile-time check for the Unity Editor, but `dotnet test` serves as the primary CI verification tool.

## Quality Assurance
We enforce strict code quality using Roslyn Analyzers and StyleCop.
*   **Tools**:
    *   `StyleCop.Analyzers`: Enforces style consistency.
    *   `SonarAnalyzer.CSharp`: Detects code smells and bugs.
*   **Commands**:
    *   `make setup`: Install dependencies.
    *   `make build`: Build the project (Mock Engine and Tests).
    *   `make test`: Run unit tests.
    *   `make format`: Auto-format code using `dotnet format`.
    *   `make lint`: Check for style violations (used in CI).
*   **Strictness**: Warnings are treated as errors in the build. Ensure your code compiles cleanly.

## Agent Instructions
*   **Deep Planning**: Please use a "Deep Planning" mode where you iteratively ask questions to clarify requirements before setting a plan.
*   **Functional Style**: Prefer functional programming patterns where applicable in C# (LINQ, immutability where sensible).
*   **Code Organization**:
    *   `Core`: Managers and singletons.
    *   `Physics`: Factories and physics helpers.
    *   `Tools`: Interaction logic.
    *   `UI`: User interface logic.
*   **Unity UI**: Use standard UGUI.
*   **Scene Setup**: handled by `SceneInitializer`. Do not edit `.unity` files directly.

## TODOs
Codebase is annotated with `TODO` comments. Please verify `TODO.md` and inline comments before starting a task.
