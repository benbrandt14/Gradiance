# Gradiance Roadmap

## Initial Features, supporting core tools
- [x] right click menu on items
- [x] select / delete items
- [x] add hinge behavior
- [ ] add fix behavior ( ie make static )
- [ ] add collision layers
- [x] add infinite plane
- [ ] add restitution and friction
- [x] add a lasso and square selection tools
- [ ] add copy/paste shortcuts by holding CTRL and dragging
- [ ] add tracers
- [x] add grid w/ locking
- [ ] add ability to modify colors ( background and objects )
- [ ] be able to modify attributes of multiple selected objects
- [ ] add cutting tool and CSG operations ( later on )
- [ ] add save/load behavior
- [ ] document anything else that bevy/avian can expose as right clickable
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

    [x] Hinge: RevoluteJoint.

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

    [ ] Undo/Redo: Command stack.

Scripting

    [ ] Console: Toggleable overlay (~ key).

    [ ] Entity Scripts: Attach.lua files to entities.

    [ ] Events: on_hit, on_spawn, on_click hooks.

    [ ] Variable Linking: entity.restitution = other.velocity.length * 0.1.

System

    [ ] Save/Load: RON file format.

    [ ] SVG Import: Drag-and-drop support.

    [ ] Performance Monitor: FPS and Body Count display.
