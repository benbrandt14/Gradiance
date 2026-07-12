# Gradiance — condensed context for brainstorming agents

A single self-contained brief so an agent can reason about **UI design, new
simulation features, and architectural trade-offs** without re-reading the
codebase. Authoritative sources it distills: `CLAUDE.md` (the enforced
contract), `docs/architecture.md` (diagrams), `docs/roadmap.md` (open work),
`docs/script-lisp-decision.md` (the scripting north star). When those and this
disagree, they win — this is a map, not the territory.

---

## 1. What it is

Gradiance is an **Algodoo-inspired 2.5D physics sandbox**: draw rigid bodies,
connect them with joints/constraints, hit play, and watch a real physics solver
run. "2.5D" = a 2D sim where a body's collision-layer bitmask doubles as render
depth, so bodies extrude into layered prisms.

The **north star** (see §7): the whole tool becomes a **DSL for multibody
simulation and geometric queries** — every menu action, tool, and sim feature is
scriptable/extensible from a `.scm` file, feeding a future
sensor/modulator/actuator dataflow layer.

- **Stack:** Bevy 0.19 ECS · Avian2d 0.7 (physics) · bevy_egui 0.41 (UI) ·
  steel (Scheme VM for the DSL). All exact-pinned. **API knowledge for these
  versions is easily stale — check `docs/bevy19-notes.md` before writing
  Bevy/avian/egui code** (Bevy 0.19 uses Messages not Events, exclusive
  systems, a 16-param system limit, etc.).
- **Toolchain:** rustc ≥ 1.95.0 (CI runs `@stable`, currently 1.97 — **stricter
  clippy than local**, e.g. `clippy::float_cmp` denies `==` on floats).
- **Gates (all must stay green):** `cargo fmt --all -- --check` ·
  `cargo clippy --all-targets -- -D warnings` · `cargo test` (includes
  `tests/boundaries.rs`, which enforces the layer rules below).

---

## 2. The five invariants (CI-enforced — violating them fails the build)

These are the load-bearing walls. Every feature is designed *around* them, and
`tests/boundaries.rs` mechanically enforces the layering ones.

1. **All world mutation goes through commands.** Tools/UI emit intent events
   (`command::intent`); only `command::dispatch` drains them, builds
   `GameCommand`s, and touches `CommandStack`. Commands mutate *authored*
   components only.
2. **Tools/UI never mutate directly.** During a drag, a tool may write transient
   preview state (a kinematic-held `Transform`, gizmos) — never authored
   components, never the stack. One gesture commits exactly one command on
   release.
3. **Identity is `StableId`; raw `Entity` is never persisted or
   cross-referenced.** (The former avian-confinement rule was retired by the
   de-adapter collapse — `docs/physics-deadapter-decision.md`. avian is used
   directly wherever physics is done; authored physics state *is* avian
   components. `physics::queries` stays as a convenience/DRY read cut-point,
   not an abstraction boundary.)
4. **`egui`/`bevy_egui` is imported only inside `src/ui/`.** UI reads component
   copies and emits intents. Sole exception: editor **settings resources**
   (`GridSettings`, `SnapConfig`, `SimSettings`) are non-authored config and may
   be written by UI; seams consume them via change detection.
5. **Authored vs derived.** Components in `src/domain/` (+ `StableId`) plus the
   authored avian components (`RigidBody`, `Friction`, `Restitution`,
   `ColliderDensity`, `GravityScale`, `Sensor`, `LockedAxes`) *are* the save
   file. Derived state (`Collider`, `Mass`, contacts, meshes, materials, live
   avian joint entities) is rebuilt by `Changed<>`-driven sync systems — never
   serialized, never in undo records, never read by commands.

**The governance model in one line:** *reads are total, writes are
seam-mediated.* Any reader (plotter, script, probe) may query any component or
resource through the read facade; every writer routes through one of three
seams — **Edit** (→ intents → commands), **Config** (→ settings resources), or
**EditorState** (→ non-sim editor tables like script actions).

---

## 3. Architecture at a glance

**One-way dataflow** (`docs/architecture.md` has the mermaid diagrams):

```
input/picking/hotkeys → tools & UI (emit intents only)
    → command::dispatch (the ONLY mutator) → CommandStack (undo/redo)
    → authored components (domain/ + StableId = the save file)
    → [Changed<>] → physics sync (colliders·joints) & render sync (meshes·materials)
    → avian solver writes Transform back → (read by tools/UI/plotters)
```

Key consequence: **loading a scene has no special cases** — spawn the authored
records and the sync systems reconstruct everything derived. Commands resolve
entities by `StableId` at execution time, so they survive undo/redo
despawn/respawn.

**Layer boundaries (compile-fenced by `tests/boundaries.rs`):**

- Pure, no ECS engine deps: `core`, `geometry`, and the `script` core
  (`kernel`, `reflect_bridge`) — put testable math here. (`domain` carries the
  authored avian components post-collapse, so it is avian-shaped by design.)
- ECS layers: `command`, `physics`, `interaction`, `render`,
  `ui` ⟨only egui⟩, `persist`, `script/bridge` ⟨only steel⟩.
- `interaction`/plotters prefer the `physics::queries` read cut-point (a
  convenience/DRY layer, no longer an enforced boundary).

**Geometry — SDF trees are the base representation.** Every shape is a
signed-distance-function tree (`ShapeDef`: `Box·Circle·Polygon·HalfPlane`
leaves; `Csg{Union,Subtract,Intersect,SmoothUnion}·Placed` nodes). **One**
discretization point, `geometry::polygonize`, turns any tree into contours, and
*every* derived consumer (colliders, meshes, snapping) reads through it. So
**cut = one `Subtract` node**, **merge = one `Union` node** — no mesh booleans.
The SDF field is also the hook for future analytic forces (magnetism, field
forces). Rationale + trade-offs: `docs/sdf-geometry-decision.md`.

**2.5D depth mapping.** `LayerMask32` memberships → `occupied_range()` →
`layer_z_range` → an extruded prism. The *same* mask is the physics collision
filter, so a body's depth and its collision set are one authored value.
`PIXELS_PER_METER = 100`; gravity `(0, -1000)`; `LAYER_HEIGHT = 10` (bit 0
front … bit 31 back).

---

## 4. Module map (`src/`)

| Module | Role |
|---|---|
| `core/` | ids (`StableId` UUID), units (`PosRot`), constants, states. Pure. |
| `domain/` | **Authored components = the save format**: `ShapeDef`, `JointDef`, `LayerMask32`, `Appearance`, settings resources. Pure. |
| `geometry/` | SDF eval, `polygonize`/contour, extrusion. Pure, no ECS — all testable math. |
| `command/` | The choke point: `intent` (typed events), `dispatch` (the sole mutator), `CommandStack`, snapshots for undo. |
| `physics/` | Sync systems derive colliders/joints from authored state; `queries` is the shared read cut-point. (avian is used directly wherever physics is done — see `docs/physics-deadapter-decision.md`.) |
| `interaction/` | Picking, selection, camera, and `tools/` (the tool facade — see §6). |
| `render/` | Derived meshes/materials, toon look, grid, joint gizmos, debug overlays. |
| `ui/` | **Only** egui import. Toolbar, inspectors, context menu, settings, script console, live plot panel. Thin projections + intents. |
| `script/` | The DSL: pure `kernel` (hot numeric tape) + `reflect_bridge`; `bridge` (the one ECS/steel seam, `run_scripts`), `registry` (operation catalog). |
| `persist/` | Single-format RON save/load of materialized authored state. |

---

## 5. Key domain concepts

- **`StableId` (UUID) on every authored entity.** Never persist or
  cross-reference a raw `Entity`; joints reference bodies by `StableId`.
- **`ShapeDef`** — the SDF tree above. Polygon vertices are centroid-relative at
  authoring time (CSG may leave the origin off-centroid).
- **Joints** — `JointDef` references two bodies by `StableId` with local
  anchors + rest rotations. `JointKind ∈ {Hinge, Weld, Slider, Spring}`; the
  physics seam derives the avian joint via `Changed<>`. `Spring` (the **strut**
  tool) maps to avian's `DistanceJoint` + `JointDamping`: `rest_length`,
  `stiffness`, `damping`, optional `range` clamp; drawn as a non-colliding coil
  gizmo. Its three scalar knobs are the ones a future curve editor would vary
  nonlinearly.
- **Tools** are `ToolContext → (preview, commit-intent)`: a `DraftTool` (draw)
  or `ManipTool` (manipulate) reads through the `ToolWorld` facade
  (`bodies_at`, `pose_of`, `shape_pose`, `id_of`), shows transient preview
  during the drag, and emits **one** `ToolCommit` (→ intent) on release.
- **Errors:** `thiserror` enums; no `unwrap`/`expect`/`panic!` outside tests
  (clippy denies).

---

## 6. Recipes (how new work slots in without breaking invariants)

These are the accretion patterns — follow them and a feature lands uniformly
instead of as a side-car.

- **A new edit / command:** add a typed intent in `command::intent` (derive
  `Reflect`, register it), build the `GameCommand` in `dispatch`, mutate only
  authored components, provide `undo`. Recipe in `src/command/mod.rs`.
- **A new tool:** implement `DraftTool`/`ManipTool` as
  `ToolContext → (preview, commit-intent)`; read via `ToolWorld`, commit one
  intent on release. Reuses the shared press/drag/release driver — the *same*
  driver a future lisp-authored tool will use.
- **A new joint/constraint:** add a `JointKind` variant + its `JointDef` fields
  (derive `Reflect`), derive the avian joint in the `physics/` sync (the only
  place avian is touched), add a gizmo in `render/joint_viz`, an inspector
  section in `ui/joint_inspector`, and a context-menu path. *Not done until the
  UI lands* — UI is part of each step, not a trailing pass.
- **A new plottable/queryable quantity:** add it to the `physics::queries`
  facade *as the feature lands*. This is the discipline that makes it free for
  plotters, probes, and scripts (a "sensor") simultaneously. The plot panel's
  history is a generic **named-signal** store — adding a signal is one line.
- **A new script verb:** add an `OpSpec` to `script/registry.rs` (name,
  signature, doc, governance **category** = Edit/Config/Query/EditorState) keyed
  by a shared `name` constant, and register the steel builtin in
  `script/bridge.rs` that emits the reflected intent / settings write / read.
  Recipe: `docs/scripting.md`. **No new mutation path** — verbs reuse the same
  three seams as tools/UI.

**The perf rule (non-negotiable for anything continuous):** the scripting VM is
the *cold/authoring* path and must **never** run per-frame. Continuous drivers
lower to the compiled, allocation-free numeric **kernel** (`script/kernel.rs`,
Tier B) that runs over query/buffer columns in one system. Bulk/particle updates
are *derived* — never commands, never undo-recorded, never persisted.

---

## 7. The scripting / DSL north star (accepted design)

Ratified in `docs/script-lisp-decision.md`: a **Lisp/DSL over a governed,
homoiconic operation registry** as the tool's control plane. Programmability is
not one milestone — it **accretes through** the substrate so we script real
features, not placeholders. Both linchpin spikes have passed:

- **Perf spike:** `script/kernel.rs` — numeric DSL → flat allocation-free tape,
  VM-free hot-path eval (~27.7 M evals/s debug at particle scale).
- **Reflect↔steel spike:** `script/reflect_bridge.rs` — one generic converter
  reads/writes any `#[derive(Reflect)]` value by reflect-path, so "everything
  programmable" is a *derive*, not N hand-written builtins.

**The three-legged dataflow vision (P2)** — this is the frame to brainstorm sim
features against:

- a **sensor** = a read over the facade (a query builtin / reflect read),
- a **modulator** = a Tier-B `kernel` over signals,
- an **actuator** = a registered Edit/Config op the signal drives.

`defsignal`/`defparam` → auto-slider; live probes/tracers/plots are just readers.
**P3** is the extension surface: user tools authored in `.scm`, data-driven
context-menu actions (first cut landed via `register-action`), and targeted
**symbolic field forces over the SDF substrate** (the flagship demo).

Persistence stays **single-format RON** of materialized authored state; the
operation registry is a runtime construct, not a second on-disk format.

---

## 8. Current state — landed vs. next

**Landed:** full command/undo core · SDF geometry + cut/merge · draw tools
(box/circle/polygon) · manip tools (select/drag) on the `ManipTool` facade ·
joints (hinge/weld/slider) + motors · **strut** spring/damper
(`JointKind::Spring`, mass-based default stiffness, coil gizmo, inspector) ·
contact-point/force debug overlay (reads avian `ContactGraph`) · **live plotter**
(selected body speed/height or joint length/angle, named-signal store) ·
**script P1**: operation registry + Edit/Config/Query/EditorState verbs + REPL
console + `--script foo.scm` loader + data-driven context-menu actions · joint
config in the right-click menu · **UI toggle buttons for the plot & script
panels** (this PR).

**Sequencing (`docs/roadmap.md` §"Sequencing after M17.1"):** substrate first,
then script it. Order: (1) interactions & joints/constraints + their UI →
(2) tracers/plotters/probes → (3) script the above (P2 dataflow) → P3 symbolic.

**Queued / open to brainstorm:**

- **Curve editor** (Lightroom-style) for nonlinear strut stiffness/damping — the
  first UI where an authored value is a *function*, not a scalar.
- More plotter signals (contact force), **pinnable multi-body probes**, and the
  script-driven `(measure …)` data-out seam.
- **P2 dataflow wiring** (`defsignal`/`defparam` → sliders/drivers) once the
  read surface is rich enough.
- **UI overhaul** — explicitly deferred: the increments above are landing
  first, then a restructure once all features are present. Discoverability,
  docking, inspector-vs-context-menu balance, and the transport strip are all
  in scope.
- M18 grids/snapping (CAD pass), M19 rendering/camera polish, M20 constraints II
  (weld-as-merge, magnetism/SDF force fields, breaking limits), M21 CSG modeling.

---

## 9. Tensions worth weighing when proposing features

- **Invariants vs. ergonomics.** Anything that "just mutates a component" is
  wrong by construction — find the seam (Edit/Config/EditorState) or propose a
  new one deliberately. The value is undo, save, and scriptability for free.
- **Authored vs. derived.** New state must be classified: is it the save file
  (authored, undoable, RON) or rebuilt-on-change (derived, never persisted)?
  Getting this wrong is the most common architectural mistake here.
- **Cold VM vs. hot kernel.** Per-frame or per-particle → kernel/derived.
  Authoring/one-shot → VM/commands. Never blur them.
- **Identity discipline.** avian types may be used directly (swappability was
  deliberately given up — `docs/physics-deadapter-decision.md`), but raw
  `Entity` must never be persisted or cross-referenced; keep the
  `physics::queries` read cut-point complete for plotters/scripts.
- **UI as thin projection.** UI holds no decisions worth testing — it reads
  copies and emits intents. A proposal that wants "smart" UI logic probably
  wants a command or a script verb instead.

**Deeper reading (only when a topic needs it):** `docs/script-lisp-decision.md`
(scripting design + governance), `docs/sdf-geometry-decision.md` (geometry),
`docs/scripting.md` (verb author guide), `docs/roadmap.md` (full milestone
list), `docs/bevy19-notes.md` (version gotchas), `docs/feature-feedback.md`
(user feedback log).
