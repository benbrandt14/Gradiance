# Assessment Report: Physics Engine Changes

## Executive Summary

We assessed two potential paths for improving physics simulation capabilities, specifically targeting multibody kinematics:
(A) Switching from Avian2d to Rapier2d.
(B) Implementing Reduced-Coordinates into Avian2d.

**Outcome**: Neither path is currently viable without significant regression or extreme effort. The attempt to switch to Rapier2d (A) was blocked by dependency incompatibilities.

## Detailed Assessment

### (A) Switching from Avian2d to Rapier2d

We attempted a "one-shot" migration to `bevy_rapier2d`.

*   **Refactoring Effort**: Moderate.
    *   Mapping components (`RigidBody`, `Collider`, `Joints`) is straightforward.
    *   Adapting `SpatialQuery` to `RapierContext` requires minor logic changes in tools.
    *   Replacing `avian2d` types with `bevy_rapier2d` types (e.g., `DVec2` to `Vec2` casting) is manageable.

*   **Blocker**: **Dependency Incompatibility**.
    *   The project uses **Bevy 0.18.0**.
    *   The latest available `bevy_rapier2d` version (0.32.0) depends on **Bevy 0.17** (specifically `bevy_ecs` 0.17).
    *   This causes compilation errors due to trait mismatches (e.g., `Component` trait is different between versions) and makes the plugins incompatible.
    *   **Conclusion**: Switching to Rapier2d is impossible without either:
        1.  Downgrading the project to Bevy 0.17 (high risk of breaking other plugins).
        2.  Waiting for or creating a fork of `bevy_rapier2d` that supports Bevy 0.18.0.

### (B) Implementing Reduced-Coordinates into Avian2d

*   **Concept**: Integrating Articulated Body Algorithms (Featherstone) or similar reduced-coordinate solvers into Avian2d.
*   **Current Architecture**: Avian2d uses Extended Position Based Dynamics (XPBD), which is a **Maximal Coordinate** solver. It solves constraints between independent rigid bodies iteratively.
*   **Effort**: **Extreme**.
    *   Implementing reduced coordinates effectively means writing a new physics solver kernel.
    *   It would require bypassing Avian's core loop or deeply modifying it to handle articulation trees differently from standard constraints.
    *   This is engine-level development, far exceeding the scope of a feature implementation.

## Recommendations

1.  **Stick with Avian2d (Current)**:
    *   Use high substeps (currently configured to 12) to improve stability of chains and linkages.
    *   Use `SolverPositionIterations` and `SolverVelocityIterations` to further stiffen constraints if needed.
    *   Accept that "perfect" multibody behavior (zero stretch) is difficult with maximal coordinates.

2.  **Future Migration**:
    *   Monitor `bevy_rapier2d` for Bevy 0.18.0 support. Once available, the migration path is clear and the refactoring work is well-understood (as attempted).

## Migration Attempt Artifacts

The migration attempt involved changes to:
*   `Cargo.toml` (Dependency swap)
*   `src/lib.rs` (Plugin setup, Ground plane)
*   `src/physics/mod.rs` (Plugin configuration)
*   `src/ui/inspector.rs` (UI Component access)
*   `src/ui/context_menu.rs` (Spatial queries)
*   `src/input/tools/*.rs` (Tool implementation)

These changes were reverted to restore a working state.
