# Gradiance architecture

A visual companion to the crate-level rustdoc (`cargo doc --open`). Every
diagram here renders on GitHub. The invariants these diagrams describe are
mechanically enforced by `tests/boundaries.rs` and CI — see `CLAUDE.md`.

## The one-way dataflow

Nothing mutates authored state except commands, and commands only run
from the dispatcher. Tools and UI are *read + emit intent*; everything
downstream of the authored components is *derived* and rebuilt on change.

```mermaid
flowchart TD
    input["input / picking / hotkeys"] --> tools["tools & UI<br/>(emit intents only)"]
    tools -- "intent messages" --> dispatch["command::dispatch<br/>(the only mutator)"]
    dispatch -- "push_apply" --> stack["CommandStack<br/>(undo / redo)"]
    stack -- "mutates" --> authored["authored components<br/>domain/ + StableId<br/>= the save file"]
    authored -- "Changed&lt;&gt;" --> physsync["physics sync<br/>colliders · joints"]
    authored -- "Changed&lt;&gt;" --> rendersync["render sync<br/>meshes · materials"]
    physsync --> solver["avian solver<br/>writes Transform back"]
    solver -. "Transform" .-> authored
    tools -. "read component copies" .- authored
```

Key consequence: **loading a scene has no special cases.** Spawn the
authored records and the sync systems reconstruct colliders, meshes, and
engine joints exactly as if the user had drawn them.

## The command lifecycle

Each edit becomes one `Box<dyn GameCommand>`. `apply` either fully
succeeds and is recorded, or fails and vanishes without a trace.

```mermaid
sequenceDiagram
    participant Tool as Tool (drag release)
    participant Disp as dispatch (exclusive)
    participant Stack as CommandStack
    participant World

    Tool->>Disp: SpawnBodyIntent
    Disp->>Disp: build Box<dyn GameCommand>
    Disp->>Stack: push_apply(cmd, world)
    Stack->>World: cmd.apply(world)
    alt Ok
        World-->>Stack: mutated
        Stack->>Stack: push to undo, clear redo
    else Err
        World-->>Stack: untouched
        Stack->>Stack: drop cmd (no history entry)
    end
    Note over Tool,World: Ctrl+Z → UndoIntent → cmd.undo(world) → redo stack
```

Commands resolve entities by `StableId` at execution time, so they stay
valid across undo/redo cycles that despawn and respawn the same body.

## Layer boundaries (compile-fenced)

```mermaid
flowchart LR
    subgraph pure["pure — no ECS engine deps"]
        core[core]
        domain[domain]
        geometry[geometry]
    end
    subgraph app["ECS layers"]
        command[command]
        physics["physics<br/>⟨only avian2d⟩"]
        interaction[interaction]
        render[render]
        ui["ui<br/>⟨only egui⟩"]
        persist[persist]
    end
    domain --> command
    geometry --> command
    command --> physics
    command --> interaction
    domain --> render
    geometry --> render
    command --> persist
    interaction -- "physics::queries facade" --> physics
    ui -- "intents only" --> command
```

`tests/boundaries.rs` scans the source and fails the build if `avian2d`
escapes `physics/`, `egui` escapes `ui/`, or `CommandStack` is named
outside `command/`. This is what lets the physics engine stay swappable
and the UI stay a thin projection.

## Geometry: SDF trees are the base representation

Every body's shape is a signed-distance-function tree. Analytic leaves
compose through CSG operators; **one** discretization point
(`geometry::polygonize`) turns any tree into polygon contours, and every
derived consumer reads through it.

```mermaid
flowchart TD
    shape["ShapeDef (SDF tree)<br/>Box · Circle · Polygon · HalfPlane<br/>Csg{Union,Subtract,Intersect,SmoothUnion} · Placed"]
    shape --> polygonize["geometry::polygonize<br/>(the single discretization point)"]
    polygonize --> contours["Contours (outline + holes)"]
    contours --> collider["physics: convex decomposition"]
    contours --> mesh["render: extrude → Mesh3d"]
    contours --> snap["interaction: snap feature points"]
    shape -. "eval() field" .-> forces["future: magnetism / SDF forces"]
```

Because CSG is just tree nodes, **cut** = one `Subtract` node and
**merge** = one `Union` node — no mesh booleans. See
`docs/sdf-geometry-decision.md` for why this representation was chosen
and its trade-offs.

## The 2.5D depth mapping

Collision-layer bits double as render depth: bit 0 is front-most, bit 31
back-most, and a body extrudes across exactly the layers it occupies.

```mermaid
flowchart LR
    mask["LayerMask32<br/>memberships = 0b0110"] --> range["occupied_range()<br/>(1, 3)"]
    range --> z["layer_z_range(1,3)<br/>z_front = -10, depth = 30"]
    z --> prism["extruded prism<br/>z ∈ [-40, -10]"]
```

The same mask is the physics collision filter, so a body's depth and its
collision set are one authored value.

## Where to start reading

- `src/lib.rs` — the crate docs (module map + this dataflow in text).
- `src/command/mod.rs` — the choke point and the "add a command" recipe.
- `src/domain/` — the authored components; this *is* the save format.
- `src/geometry/sdf.rs` and `contour.rs` — the geometry core, with
  runnable examples.
- `docs/sdf-geometry-decision.md`, `docs/roadmap.md`,
  `docs/feature-feedback.md` — the design decisions and open work.
