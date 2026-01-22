# Architecture Audit & Assessment

**Date:** 2024-05-23
**Project:** Gradiance
**Auditor:** Jules (AI Software Architect)

## 1. Executive Summary

The **Gradiance** codebase demonstrates a solid understanding of the Bevy engine's Entity-Component-System (ECS) architecture. The project is well-structured with clear separation of concerns between Physics, Input/Tools, and UI. The modular plugin design (`InputPlugin`, `UiPlugin`, `GamePlugin`) promotes maintainability.

However, two **critical architectural flaws** were identified that jeopardize the stability and core functionality of the editor:
1.  **Fragile Undo/Redo:** The current Command pattern relies on ephemeral `Entity` IDs. Redoing a creation command generates a new Entity ID, breaking any dependent commands (like Joints) that reference the old ID.
2.  **Bypassed History:** The Property Inspector modifies components directly, completely bypassing the `CommandStack`. This means property changes cannot be undone, leading to inconsistent editor state.

## 2. Structural Analysis

### 2.1 File Organization
The project follows a standard Rust/Bevy directory structure:
*   `src/lib.rs`: Clean entry point, aggregates plugins.
*   `src/input/`: effectively acts as the "Controller" in MVC terms.
    *   `tools/`: Excellent separation of tool logic. The "State Machine" pattern (`ToolState`) is correctly implemented.
    *   `commands/`: The Command pattern is present but structurally incomplete (see Section 3.1).
*   `src/ui/`: Standard `bevy_egui` implementation.
*   `src/geometry/` & `src/physics/`: Appropriate domain encapsulation.

### 2.2 ECS Usage
*   **Systems:** Logic is generally well-distributed into systems.
*   **Queries:** Usage is idiomatic.
*   **Resources:** Effective use of Resources for global state (`Selection`, `CommandStack`, `ToolState`).
*   **States:** `ToolState` is used effectively to toggle systems.

## 3. Critical Findings

### 3.1 Fragile Undo/Redo Integrity (Severity: High)
**The Issue:**
The `CommandStack` relies on Bevy's `Entity` (a generational index) to reference objects. When a `SpawnBoxCommand` is undone, the entity is despawned. When it is *redone*, a **new** entity is spawned with a new generation ID (and potentially new index).
Any subsequent command in the stack (e.g., `SpawnRevoluteJointCommand`) that references the *original* Entity ID will fail or panic when redone, as it points to a dead or non-existent entity.

**Evidence:**
*   `src/input/commands/shapes.rs`: `undo_despawn_recursive` destroys the entity. `apply` creates a new one via `world.spawn`.
*   `src/input/commands/joints.rs`: `SpawnRevoluteJointCommand` stores `entity_a: Entity`. This field is never updated when the referenced entity is regenerated.

### 3.2 Inspector Bypasses Command History (Severity: High)
**The Issue:**
The Inspector UI (`src/ui/inspector.rs`) directly iterates over selected entities and mutates components (e.g., `transform.translation = ...`).
Because these mutations occur outside the `CommandStack`, they cannot be undone. Furthermore, mixing "Undoable" actions (creation) with "Non-Undoable" actions (editing) in the same session renders the Undo stack unreliable.

**Evidence:**
*   `src/ui/inspector.rs`: `if ui.add(...).changed() { *b = local_box; }`. Direct assignment to mutable query results.

## 4. Minor Findings

### 4.1 UI/Logic Coupling
The UI systems occasionally contain game logic. For example, `inspector_ui` contains logic for determining which components to show and how to interpret them. While common in immediate mode GUIs, it makes the UI harder to test.

### 4.2 Ambiguous "Entity" Ownership
Commands store `Option<Entity>` to track if they have spawned something. This ownership model is implicit. If an entity is deleted by a *different* mechanism (e.g., a "Delete Tool"), the Command Stack might hold references to dead entities, confusing the history.

## 5. Architectural Recommendations

### 5.1 Recommendation A: Stable Identifiers (Solves 3.1)
**Proposal:** Introduce a `StableID` (UUID) component.
1.  **Component:** Add `#[derive(Component)] struct StableID(Uuid);`
2.  **Resolution Resource:** Maintain a `Resource` mapping `HashMap<StableID, Entity>`.
3.  **Command Refactoring:** Commands should store `StableID` instead of `Entity`.
    *   `SpawnBoxCommand` generates a `StableID` on creation.
    *   On `apply`, it spawns the entity and registers `StableID -> Entity` in the resource.
    *   `SpawnJointCommand` looks up the fresh `Entity` using the stored `StableID`.

### 5.2 Recommendation B: Property Mutation Commands (Solves 3.2)
**Proposal:** The Inspector should never mutate data directly.
1.  **Generic Command:** Create `ModifyComponentCommand<C: Component>`.
    *   Stores `entity: StableID`, `old_value: C`, `new_value: C`.
2.  **Inspector Update:** When a UI widget changes:
    *   Capture the previous value.
    *   Push a `ModifyComponentCommand` to the `CommandStack`.
    *   This ensures all property edits are undoable.

### 5.3 Recommendation C: Event-Driven UI
**Proposal:** Decouple UI from World mutation.
1.  UI systems emit events (e.g., `EventWriter<ChangeColorEvent>`).
2.  A separate system (`handle_color_changes`) reads events and pushes Commands to the stack.
3.  This makes it easier to script the game or drive it from tests without spinning up the UI.

## 6. Implementation Roadmap

1.  **Phase 1 (Foundation):** Implement `StableID` and the resolution map. Refactor `Spawn` commands to register IDs.
2.  **Phase 2 (Migration):** Refactor `Joint` commands to use `StableID`. Verify "Undo Box -> Redo Box -> Redo Joint" works.
3.  **Phase 3 (Completeness):** Implement `PropertyChangeCommand` and rewire the Inspector.
