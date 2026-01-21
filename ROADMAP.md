# Gradiance Roadmap

## Initial Features, supporting core tools
- [x] right click menu on items
- [x] select / delete items
- [x] add hinge behavior
- [ ] add fix behavior ( ie make static )
- [ ] add collision layers
- [ ] add infinite plane tool
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

    [ ] Plane Tool: Spawns static infinite half-space. (Implemented as startup system, needs tool)

    [x] Box Tool: Spawns Collider::cuboid.

    [x] Circle Tool: Spawns Collider::ball.

    [x] Polygon Tool: Click-to-place vertices, close loop to spawn.

    [ ] Brush/Sketch Tool: Freehand draw -> simplify -> polygon.

    [ ] Cut Tool: CSG difference operation on World geometry.

    [x] Drag Tool: MouseJoint implementation.

    [ ] Scale/Rotate Tool: Gizmos for transforming entities (use bevy_transform_gizmo).

    [x] Select Tool: Box selection and Move functionality.

Physics & Constraints

    [x] Hinge: RevoluteJoint ( Half Implemented )

    [ ] Fixed: FixedJoint (Weld).

    [ ] Spring: DistanceJoint with soft compliance.

    [ ] Slider: PrismaticJoint with limits.

    [ ] Chain: Procedurally generated linked bodies.

    [ ] Rope (Pulley): Custom constraint (non-colliding length constraint).

    [ ] Gear: Custom constraint (angular velocity ratio).

    [ ] Collision Layers: UI to toggle collision masks (A collides with B).

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

    [x] Undo/Redo: Command stack (Architecture implemented, Deletion pending).

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

    [ ] Implement Camera2dBundle with custom Pan/Zoom system (Orthographic scale).

        Constraint: Zoom must center on mouse cursor.

    [ ] Implement InfiniteFloor system:

        Spawn Collider::half_space at y=0.

        Render infinite grid using a custom shader (or bevy_infinite_grid fork) that scales lines based on zoom level.

    [ ] Implement Selection resource using bevy_mod_picking:

        Click to select single.

        Shift+Click to multi-select.

        Drag background to box-select (AABB query).

    [ ] Implement MouseJoint (The "Hand" Tool):

        On drag start: Spawn DistanceJoint (high stiffness) between cursor and object.

        On drag update: Update joint anchor to mouse position.

Phase 2: Geometry & CSG Pipeline (Months 3–4)

Goal: The "Sketch" and "Cut" tools. Creating custom shapes and slicing them.

Milestones:

    Vector Rendering: High-fidelity shape drawing.

    CSG Kernel: Boolean operations (Cut, Weld).

    Tessellation: Polygon to Mesh/Collider.

Tasks:

    [ ] Integrate bevy_prototype_lyon:

        Create VectorMesh component (holds path data separate from Bevy Mesh).

    [ ] Implement Sketch Tool:

        Capture points -> Ramer-Douglas-Peucker simplification -> Lyon Path.

    [ ] Integrate clipper2 crate:

        Implement utility fn to_clipper_path(Vec<Vec2>) -> Vec<Point64>.

        Implement scaling factor (105) to handle float-to-int conversion.

    [ ] Implement Cut Tool (Laser):

        Input: Line segment A→B.

        Expand line to thin polygon Pcut​.

        Query intersecting bodies.

        Perform Difference(Body, P_{cut}).

        Despawn old body, spawn new bodies.

        Crucial: Calculate new Center of Mass and offset children/collider/mesh to local (0,0).

    [ ] Implement Polygon Decomposition:

        Use parry2d's convex decomposition on resulting shapes to generate valid Rapier colliders.

Phase 3: The Mechanical Engineer (Months 5–6)

Goal: Advanced constraints. Gears, Pulleys, and Linkages.

Milestones:

    Standard Joints: Hinge, Fixed, Spring.

    Custom Solvers: Gear and Pulley constraints (Math-heavy phase).

    Visualization: Seeing the joints.

Tasks:

    [ ] Implement Hinge Tool:

        Spawn RevoluteJoint.

        Visuals: Draw a "Bolt" sprite at anchor.

    [ ] Implement Spring/Slider Tool:

        Spawn PrismaticJoint (Slider) or DistanceJoint (Spring).

        UI: Sliders for Stiffness, Damping.

    [ ] Custom Constraint: Gear Joint:

        Implement GeneralizedConstraint trait in Rapier (if supported).

        Constraint Eq: ΔθA​+rΔθB​=0.

        Add Backlash parameter (dead zone before constraint engages).

    [ ] Custom Constraint: Pulley:

        Implement "Rope" logic: Distance(A, AnchorA) + Distance(B, AnchorB) <= Length.

        Visuals: Draw lines from A→AnchorA→AnchorB→B.

        Stretch: Allow wire to wrap around circular obstacles (requires Raycast/ConvexCast wrapping algorithm).

    [ ] Chain Tool:

        Procedural generation: Spawn N small capsule bodies linked by RevoluteJoints.

        Tune SolverIterations (substeps) to preventing chain explosion.

Phase 4: Simulation & Scripting (Months 7–8)

Goal: Fluids, Lasers, and Scripting terminal.

Milestones:

    Fluids: SPH Implementation.

    Optics: Lasers and refraction.

    Scripting: Lua integration.

Tasks:

    [ ] SPH Fluid System (CPU):

        Implement Spatial Hash Grid resource.

        Particle Entity: Position, Velocity, FluidParticle.

        Solver: Density constraint ρi​=∑W(rij​). Pressure force −∇p.

        Coupling: Raycast from particles to Rapier bodies. Apply impulse to body; reflect particle.

    [ ] Laser/Optics System:

        Recursive Raycast system.

        Materials: RefractiveIndex, Reflectivity.

        Visuals: Bloom mesh using bevy_firefly.

    [ ] Scripting Integration:

        Integrate bevy_mod_scripting with Lua.

        Create "Half-Life" style console using bevy_egui.

        Bind Entity queries to Lua (e.g., scene.get("box1").color = "red").

        Implement ScriptComponent that runs on_update(dt) hook.

Phase 5: The "Algodoo" Polish (Months 9–10)

Goal: UI, Plotting, and Quality of Life.

Milestones:

    Material Manager: Grouping attributes.

    Property Inspector: The main UI.

    Graphing: Real-time plots.

Tasks:

    [ ] Inspector UI (bevy_egui):

        Selection-driven panel.

        Reflection-based auto-UI for components (Friction, Restitution, Mass).

        Color picker (rgba).

    [ ] Material Profiles:

        Resource MaterialLibrary (Gold, Ice, Rubber).

        Applying profile batch-updates components on selected entities.

    [ ] Plotting/Tracers:

        Component Tracer { duration, color }.

        System: Record position history, render using bevy_polyline.

        Mini-graph UI: Plot velocity magnitude vs time for selected object.

Phase 6: Extension & Release (Months 11–12)

Goal: Save/Load, Lighting, Optimization.

Milestones:

    Persistence: Serialization.

    Lighting: 2D Shadows.

    Optimization: Multithreading.

Tasks:

    [ ] Save/Load:

        Integrate moonshine_save.

        Serialize Rapier components and Lyon paths to RON.

        Handle Entity ID remapping for Joints.

    [ ] Lighting:

        Integrate bevy_firefly.

        Add ShadowCaster component to all RigidBodies.

    [ ] Fracturing (Beta):

        System: On CollisionEvent, if Impulse > Threshold:

        Trigger Voronoi shatter (using delaunator or similar) -> Replace body with shards.
