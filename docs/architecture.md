# Gradiance architecture

A visual companion to the crate-level rustdoc (`cargo doc --open`). Every
diagram here renders on GitHub. The invariants these diagrams describe are
mechanically enforced by `tests/boundaries.rs` and CI — see `CLAUDE.md`.

## State classification

Every resource/component in `src/`, by the seam governing its writes (reads
are total, writes are seam-mediated). New state must land in exactly one
section; anything that doesn't fit gets an explicit rationale row at the
bottom — no unclassified state.

### Authored (Edit seam → persisted, undoable, reflect-registered)

| State | Kind | Example / notes |
|---|---|---|
| `StableId` | identity component | on every authored entity; the cross-reference key |
| `Body`, `Joint` | marker components | classify an authored entity |
| `ShapeDef` | component | SDF tree; reflect-opaque leaf |
| `DepthBand` | component | continuous depth band = collision volume *and* render depth |
| `Appearance` | component | fill / emissive |
| `JointDef` | component | kind + anchors + rest rotations, bodies by `StableId` |
| `SelectionGroup` | component | group membership (persisted, undoable via commands) |
| `Transform` (x, y, θ) | component | authored pose, captured as `PosRot`; z/scale derived |
| avian `RigidBody`, `Friction`, `Restitution`, `ColliderDensity`, `GravityScale`, `Sensor`, `LockedAxes` | components | authored physics **is** avian components (de-adapter) |
| `FieldSource` | component | field source (signed repulsion, falloff); sampled only via `physics::fields::Fields` (`docs/field-architecture.md`) |
| `Tracer` | component | trajectory-trail marker (on a body, or on a node) |
| `BehaviorNode`, `NodeAttachment`, `NodeKind` | components | placeable dataflow node (tracer / sensor / actuator tools) — `StableId` + pose + optional body attachment, saved in `SceneRecord.nodes`; kind edited via `PropertyValue::NodeKind` (`docs/signal-dataflow.md`) |
| `Tracer` | component | trajectory-trail marker (fade window); the sampled trail is derived (`TraceTrail`) |

### Config (settings resources — UI writes directly; invariant-4 exception)

| State | Persisted? | Undoable? | Reflect? | Notes |
|---|---|---|---|---|
| `GridSettings`, `SnapConfig` | scene file | no | yes | grid/snap setup travels with the scene |
| `SimSettings` | scene file | no | yes | applied by `apply_sim_settings` on change; `sim-set` writes it |
| `RenderSettings` | scene file | no | yes | consumed by `toon::apply_render_settings` |
| `LightingSettings`, `ScenerySettings` | scene file | no | yes | applied by `render::scenery` (key light, ambient, SSAO, back plane) and `render::ground` (ground visibility) |
| `DebugSettings` | **no** | no | yes | workstation overlays, not scene state |
| `ToolDefaults` | **no** | no | yes | tool-creation defaults (slider travel limits); consulted at gesture commit time |
| `SignalBindings`, `SignalParams`, `ComputedSignals` | scene file | no | yes | the signal-dataflow graph (`docs/signal-dataflow.md`): source→sink bindings, `defparam` knobs, `defsignal` modulators; evaluated per frame by `signal::evaluate_computed`/`evaluate_signals` |

### EditorState (non-authored editor tables; not persisted, not undoable)

| State | Notes |
|---|---|
| `ScriptActions` | `register-action` table the context menu surfaces |
| `WorkspaceLabels` | script-given body names (`label`); viewport tags + pick-list names |
| `ScriptInputs`, `StartupScripts`, `ScriptWatch`, `ScriptLog` | script doorway queues + console log |
| `OperationRegistry`, `IntentDispatch` | runtime catalog + intent-bus wiring (static after startup) |
| `LastScenePath`, `AutosavePath`, `StartupScene` | persistence bookkeeping |
| `Selection`, `SelectedJoint` | current selection (entities, never saved) |
| `GameState`, `ToolState` | bevy states: play/pause, active tool |
| `ScaleFrame` | global/local handle axes toggle (F) |
| UI panel state: `SettingsWindow`/`SettingsTab`, `InspectorPanel`, `ContextMenu`, `PlotPanel`, `ProbePanel`, `SignalsPanel`, `ScriptConsole`, `DepthPanel` | open/closed + per-panel scratch (incl. the depth dock's in-flight bar drag); egui-side only (`ProbePanel` pins bodies by `StableId`, so undo/redo keep pins valid) |

### Transient gesture/preview state (tool-local; invariant 2 — dies with the gesture)

| State | Notes |
|---|---|
| draft tools: `BoxTool`, `CircleTool`, `CutTool`, `GroundTool`, `PolygonTool`, `ConnectorDraft`, `StrutDraft` | anchor points while drawing; commit one intent on release |
| `DragTool`, `SelectGesture`, `ActiveGesture`, `ClickThrough`, `GestureConstraints`, `JointAnchorDrag`, `SuppressSelectPress` | manipulation gesture state |
| `KinematicHold` | kinematic-held pose during a drag (the sanctioned preview write) |
| `MouseSpring`, `MouseTwist` | play-mode grab forces (physics-frame transient) |
| `CursorWorldPos`, `SnappedCursor`, `SnapExclusions`, `PointerButtons`, `PointerOverUi`, `KeyboardCaptured` | per-frame input/picking snapshots |
| `CameraRig` | editor camera pan/zoom (workstation state) |

### Derived (rebuilt by `Changed<>` sync; never serialized, never in undo)

| State | Rebuilt by |
|---|---|
| `Collider`, `CollisionLayers`, `Mass`, contacts | `body_sync` (+ avian internals) |
| live avian joints (`RevoluteJoint`, `PrismaticJoint`, `DistanceJoint`, `JointDamping`), `JointUnresolved`, `PinAnchor` | `joint_sync` |
| `Mesh3d`, `MeshMaterial3d`, `ToonMaterial` | `extrude_sync`, `material_sync` |
| `FieldMass` | `fields::sync_field_mass` (shape area × density — the field-coupling mass, `docs/field-architecture.md`) |
| `TraceTrail` | `render::tracer::sample_traces` (physics-clock position window behind the authored `Tracer`, on a body or a node; removed with its marker) |
| attached node `Transform` | `render::tracer::follow_node_attachments` (an attached behavior node's pose re-derived each frame from its target body) |
| `SignalBus`, `ScriptSignals` | `signal::evaluate_signals` + script `signal-set` (named values + rolling histories; bus hygiene drops unbound names) |
| `CompiledSignals` | `signal::recompile_signals` (compiled kernels behind `ComputedSignals`, rebuilt on change — keeps the kernel *compile* off the frame loop) |
| `SignalColorOverride` | `signal::evaluate_signals` (derived tint preferred by `material_sync`/`tracer` over authored `Appearance`; removed with its binding) |
| `IdIndex` | `StableId` component hooks |
| `HistoryInfo` | dispatcher (read-only mirror of stack depths) |
| `CameraScale` | `apply_camera_rig` (world-per-pixel under either projection) |
| `KeyLight` / `BackPlane` entities | `render::scenery` from `LightingSettings`/`ScenerySettings` |
| `SubstepTrace` | `record_substep_trace` in avian's `SubstepSchedule` (debug view; rebuilt every physics step while enabled) |

### Doesn't fit cleanly (explicit rationale)

| State | Classification | Rationale |
|---|---|---|
| `CommandStack` | the undo history itself | neither authored nor derived: it *produces* authored state. Private to `command/`; lost on exit by design (history is session state) |
| `PlotHistory` | derived-but-accumulating | a pure read of physics state, but it accumulates samples over time, so it can't be rebuilt from the current frame. Still never persisted/undone; cleared on retarget |

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

## Layer boundaries (package-fenced)

Since the workspace split (`docs/workspace-plan.md`), every layer is its own
package under `crates/`, and the diagram below **is** the dependency graph
in the manifests — an edge that isn't declared does not compile. Arrows
point from a package to what it depends on:

```mermaid
flowchart TD
    kernel["kernel<br/>(pure, no bevy)"]
    core[core]
    geometry["geometry<br/>(SDF shape tree + math)"] --> core
    domain["domain<br/>(authored, avian-shaped)"] --> core
    domain --> geometry
    scene["scene<br/>(records + RON format)"] --> domain
    physics --> domain
    signal --> kernel
    signal --> physics
    command --> scene
    command --> signal
    persist --> command
    interaction --> persist
    render -- "editor-state viz" --> interaction
    script["script<br/>⟨only steel⟩"] --> command
    script --> signal
    ui["ui<br/>⟨only egui⟩"] --> interaction
    ui --> script
```

`egui` is a dependency of `gradiance-ui` only and `steel` of
`gradiance-script` only; `tests/boundaries.rs` re-checks the source as text
(so a manifest drift is caught in review too), asserts serde stays confined
to authored/persisted data, holds the exact-pin line on engine
dependencies, and keeps `CommandStack` named only inside the command
package. (The avian-confinement rule was retired by the de-adapter
collapse — `docs/physics-deadapter-decision.md`: avian is used directly
wherever physics is done, and `physics::queries` is a convenience/DRY read
cut-point, not an abstraction boundary.) This keeps the UI a thin
projection and the scripting seam single.

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

A body authors one continuous `DepthBand { near, far }` (world units into
the screen). The extrusion uses the floats directly; the collision filter
derives contiguous layer bits from the same band — so depth and collision
set are one authored value and non-integer depths are first-class.

```mermaid
flowchart LR
    band["DepthBand<br/>near = 5, far = 23"] --> prism["extruded prism<br/>z ∈ [-23, -5]"]
    band --> bits["bits() → 0b0111<br/>(layers 0..=2 overlapped)"]
    bits --> cl["CollisionLayers<br/>memberships = filters = bits"]
```

Two bodies collide exactly when their bands overlap (at layer
granularity); ground half-planes collide with everything. There is no
separate filter mask — checkbox filter art was retired with save v5
(v4 masks migrate to the equivalent band on load).

## Where to start reading

- `src/lib.rs` — the app-shell crate docs (package map + this dataflow in
  text).
- `crates/gradiance-command/src/lib.rs` — the choke point and the "add a
  command" recipe (one `command_intents!` table row per command).
- `crates/gradiance-domain/` — the authored components (the save
  *content*); `crates/gradiance-scene/` — the records and RON format (the
  save *file*).
- `crates/gradiance-geometry/src/sdf.rs` and `contour.rs` — the geometry
  core, with runnable examples.
- `docs/workspace-plan.md`, `docs/sdf-geometry-decision.md`,
  `docs/roadmap.md`, `docs/feature-feedback.md` — the architecture
  rationale, design decisions, and open work.
