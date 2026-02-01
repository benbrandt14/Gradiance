# Gradiance Roadmap

## Initial Features, supporting core tools
- [x] right click menu on items
- [x] select / delete items
- [x] add hinge behavior
- [x] add fix behavior ( ie make static )
- [ ] add collision layers
- [ ] add infinite plane tool (Currently approximated with large box)
- [ ] add restitution and friction
- [x] add square selection tool
- [ ] add a lasso selection tool
- [ ] add multiselect with shift+click
- [ ] add copy/paste shortcuts by holding CTRL and dragging
- [ ] add tracers
- [x] add grid w/ locking
- [ ] add ability to modify colors ( background and objects )
- [ ] be able to modify attributes of multiple selected objects
- [ ] add cutting tool and CSG operations ( later on )
- [ ] add save/load behavior
- [ ] document anything else that bevy/Rapier2d can expose as right clickable
- [ ] mimic UI style of algodoo ( icons can be provided for some things )
- [ ] add lasers / optics behavior
- [ ] add translucency
- [ ] add attraction/repulsion
- [ ] be able to modify density & other physical attributes ( friction )
- [ ] add sketch tool
- [ ] add CAD style constraint-based sketching (much later)
- [ ] add drag-and-drop SVG support (much later)
- [ ] add particle-system type behaviors ( in a consistent way)
- [ ] add rotation around points and other greebles
- [ ] add distribute array-like physical alignment
- [ ] add tweening and other animation polish
- [ ] add some lighting / 2D shadow casting with shaders
- [ ] add rockets & sliders
- [ ] add fine control of constraints / damping / rotation limits / for nearly everything
- [ ] add scripting support with a popup ( half-life style ) terminal
- [ ] be able to control parameters by linking them to others ( ie restitution based on proximity ) + support math / functional notation in menus + visualize linkage between parameters (later on)
- [x] add play/pause button with stepping control and sim time control
- [ ] add sensors for proximity and force, make them thematically match ( proximity might be capacitance )
- [ ] add a settings pane for things to be modified
- [ ] add support for chains and pulleys ( esp. physically correct pulleys w/ non colliding wires -- not part of original algodoo and easy to add )
- [ ] add support for arbitrary coordinate frames ( req plotters )

## Core Tools ( Polish & Feature Complete )

    [x] Plane Tool: Spawns static ground (Approximated). Needs infinite shader.

    [x] Box Tool: Spawns Collider::cuboid.

    [x] Circle Tool: Spawns Collider::ball.

    [x] Polygon Tool: Click-to-place vertices, close loop to spawn.

    [ ] Brush/Sketch Tool: Freehand draw -> simplify -> polygon.

    [ ] Cut Tool: CSG difference operation on World geometry.

    [x] Drag Tool: MouseJoint implementation. (Needs offset fix)

    [ ] Scale/Rotate Tool: Gizmos for transforming entities (use bevy_transform_gizmo).

    [x] Select Tool: Box selection and Move functionality.

Physics & Constraints

    [x] Hinge: RevoluteJoint (Implemented via ConnectorTool).

    [x] Fixed: FixedJoint (Weld) (Implemented via ConnectorTool).

    [ ] Spring: DistanceJoint with soft compliance.

    [ ] Slider: PrismaticJoint with limits.

    [ ] Chain: Procedurally generated linked bodies.

    [ ] Rope (Pulley): Custom constraint (non-colliding length constraint).

    [ ] Gear: Custom constraint (angular velocity ratio).

    [ ] Collision Layers: UI to toggle collision masks (A collides with B). (Critical for Pinning)

    [ ] Material Properties: Friction, Restitution (Bounciness), Density.

Fluid & Particles

    [ ] Liquify: Convert rigid body to particle cluster.

    [ ] SPH Solver: Density/Pressure calculation.

    [ ] Two-way Coupling: Particles push bodies; bodies push particles.

    [ ] Buoyancy: Upward force on bodies intersecting fluid zone.

    [ ] Soft Body: Convert Rigid Body to Soft

Visuals

    [ ] Tracers: Fade-out path visualization.

    [ ] Lighting: Point lights, shadow casting (bevy_firefly).

    [ ] Color: RGBA support with alpha blending.

    [ ] Optics: Transmission and reflection of light.

UI & UX

    [x] Context Menu: Right-click entity to show actions.

    [ ] Inspector: Sidebar showing properties of selected object.

    [x] Grid: Snapping and visual grid.

    [x] Play/Pause: Global time scale control.

    [ ] Step: Advance simulation 1 tick.

    [x] Camera Controller: Pan and Zoom.

    [x] Undo/Redo: Command stack (Architecture implemented).

Scripting

    [ ] Console: Toggleable overlay (~ key).

    [ ] Entity Scripts: Attach.lua files to entities.

    [ ] Events: on_hit, on_spawn, on_click hooks.

    [ ] Variable Linking: entity.restitution = other.velocity.length * 0.1.

System

    [ ] Save/Load: RON file format.

    [ ] SVG Import: Drag-and-drop support.

    [ ] Performance Monitor: FPS and Body Count display.

## Future Roadmap (Design Goals)

Phase 1: The Substrate (Months 1–2)

Goal: A stable infinite 2D world where you can spawn rigid bodies and move the camera.

Milestones:

    Core scaffolding: Bevy 0.15.3 app structure with Rapier2d.

    Input/Camera: Pan/Zoom camera and Mouse Picking.

    The Floor: Infinite plane implementation.

Tasks:

    [x] Initialize project with bevy, Rapier2d, bevy_egui.

    [x] Implement Camera2dBundle with custom Pan/Zoom system (Orthographic scale).

    [x] Implement InfiniteFloor system (Approximated):
        Spawn Collider::cuboid (Huge) at y=0.
        *TODO*: Render infinite grid using a custom shader.

    [x] Implement Selection resource:
        Click to select single.
        Shift+Click to multi-select.
        Drag background to box-select.

    [x] Implement MouseJoint (The "Hand" Tool):
        On drag start: Spawn DistanceJoint.
        *Issue*: Offset calculation needs fixing.

Phase 2: Geometry & CSG Pipeline (Months 3–4)

Goal: The "Sketch" and "Cut" tools. Creating custom shapes and slicing them.

Milestones:

    Vector Rendering: High-fidelity shape drawing.

    CSG Kernel: Boolean operations (Cut, Weld).

    Tessellation: Polygon to Mesh/Collider.

Tasks:

    [x] Integrate bevy_prototype_lyon:
        Verified working with 0.13.0.

    [ ] Implement Sketch Tool:
        Capture points -> Ramer-Douglas-Peucker simplification -> Lyon Path.

    [ ] Integrate clipper2 crate:
        Implement utility fn to_clipper_path(Vec<Vec2>) -> Vec<Point64>.

    [ ] Implement Cut Tool (Laser):
        CSG operations on bodies.

    [ ] Implement Polygon Decomposition:
        Use parry2d's convex decomposition.

Phase 3: The Mechanical Engineer (Months 5–6)

Goal: Advanced constraints. Gears, Pulleys, and Linkages.

Milestones:

    Standard Joints: Hinge, Fixed, Spring.

    Custom Solvers: Gear and Pulley constraints.

Tasks:

    [x] Implement Hinge Tool (ConnectorTool).

    [x] Implement Fixed Tool (ConnectorTool).

    [ ] Fix Pin Collision:
        Ensure pinned bodies do not collide with their pin anchor (Explosion risk).
        *Status*: Needs Fix (See TECH_DEBT.md).

    [ ] Implement Spring/Slider Tool.

    [ ] Custom Constraint: Gear Joint.

    [ ] Custom Constraint: Pulley.

    [ ] Chain Tool.

(Phases 4-6 remain unchanged)

## Maintenance & Refactoring
- [ ] **Technical Debt**: Address items in `TECH_DEBT.md`.
    - [ ] Pin Collision Instability.
    - [ ] Drag Tool Offset.
    - [ ] Inspector Query Simplification.
- [ ] **Documentation**: Maintain `AGENTS.md` and `RUST_GUIDELINES.md`.

## Technical Debt & Issues
See `TECH_DEBT.md` for detailed analysis of architectural hurdles and known issues.
