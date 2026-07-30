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
4. **`egui`/`bevy_egui` is a dependency of `crates/gradiance-ui` only** (and
   `steel` of `crates/gradiance-script` only) — enforced by the package graph
   and re-checked by `tests/boundaries.rs`. UI reads component copies and emits
   intents. Sole exception: editor **settings resources** may be written by UI;
   seams consume them via change detection. Those split by *what they describe*:
   **scene content** (`SimSettings`, `RenderSettings`, `LightingSettings`,
   `ScenerySettings`, the signal-graph resources) is part of the document —
   saved **and** undoable, one settled edit per drag; **workstation config**
   (`GridSettings`, `SnapConfig`) belongs to the person — saved, never in undo
   history. The split lives in `scene::records::EnvironmentRecord`.
5. **Authored vs derived.** Components in `gradiance-domain` (+ `StableId`) plus the
   authored avian components (`RigidBody`, `Friction`, `Restitution`,
   `ColliderDensity`, `GravityScale`, `Sensor`, `LockedAxes`) *are* the save
   file. Derived state (`Collider`, `Mass`, contacts, meshes, materials, live
   avian joint entities) is rebuilt by `Changed<>`-driven sync systems — never
   serialized, never in undo records, never read by commands. The save
   *format* (records, RON, version migrations) is `gradiance-scene`; records
   are the shared unit of undo capture and persistence.

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

**Layer boundaries — the layer diagram *is* the crate DAG.** Each architectural
layer is its own package under `crates/`, so a boundary violation is a **compile
error**, not a review comment. `tests/boundaries.rs` asserts the DAG itself as
data: adding a `gradiance-*` dependency edge needs a matching row there, which
makes it a deliberate, reviewed architecture change.

```text
kernel → (nothing)                  units → (nothing, +bevy for Reflect)
core → bevy                         geometry → core
domain → core, geometry             scene → core, domain
optimize → core, geometry           physics → core, domain, geometry, units
signal → kernel, domain, physics     command → scene, signal (+ lower)
persist → scene, command            interaction → command, optimize, persist (+ lower)
render → interaction, units (+ lower)   script → command, signal (+ lower)
ui → everything except render
```

- Fully pure (no bevy at all): `kernel`. No systems/resources/queries, bevy only
  for `Reflect`/`Component` derives: `geometry`, `optimize`, `units` — put
  testable math there. (`domain` carries the authored avian components
  post-collapse, so it is avian-shaped by design.)
- `interaction`/plotters prefer the `physics::queries` read cut-point (a
  convenience/DRY layer, no longer an enforced boundary).
- Rationale and the roadmap→package feature tree: `docs/workspace-plan.md`.

**Geometry — SDF trees are the base representation.** Every shape is a
signed-distance-function tree (`ShapeDef`: `Box·Circle·Polygon·HalfPlane`
leaves; `Csg{Union,Subtract,Intersect,SmoothUnion}·Placed` nodes). **One**
discretization point, `geometry::polygonize`, turns any tree into contours, and
*every* derived consumer (colliders, meshes, snapping) reads through it. So
**cut = one `Subtract` node**, **merge = one `Union` node** — no mesh booleans.
The SDF field is also the hook for future analytic forces (magnetism, field
forces). Rationale + trade-offs: `docs/sdf-geometry-decision.md`.

**2.5D depth mapping.** Depth is a continuous authored `DepthBand {near, far}`
(world units into the screen); a body spans `z ∈ [−far, −near]` and extrudes
into a prism. The collision **layer bits** (`LAYER_HEIGHT = 10.0` slabs, bit 0
front) are *derived* from the band, so collision layer ≡ visual depth — one
authored value, not two that can disagree. `PIXELS_PER_METER = 100`; gravity
`(0, -1000)`. `gradiance-units` owns the single px↔SI seam
(`docs/units-decision.md`).

---

## 4. Package map (`crates/`)

The root package `gradiance` is the app shell (plugin group, prelude, `main`,
the integration tests) and re-exports each package under its layer name
(`gradiance::command`, `gradiance::geometry`, …), so doc and test paths read as
they did before the split.

| Package | Role |
|---|---|
| `gradiance-kernel` | The Tier-B numeric tape: `Expr` → flat allocation-free `Kernel`, plus `Lut` (sampled response curves). Fully pure — no bevy. |
| `gradiance-units` | Typed SI quantities and the one px↔SI seam (`units::world`). |
| `gradiance-core` | ids (`StableId` UUID), `PosRot`, constants, states. |
| `gradiance-geometry` | SDF eval, `polygonize`/contour, extrusion, hulls/SAT, array pitch. No systems, no queries — all testable math. |
| `gradiance-domain` | **Authored components = the save content**: `ShapeDef`, `JointDef`, `DepthBand`, `Appearance`, signal types, settings resources. |
| `gradiance-scene` | The save **format**: records, RON, version migrations. Records are the shared unit of undo capture and persistence. |
| `gradiance-optimize` | The layout solver: hulls, SAT, a weighted objective, and a `Solver` trait. Pure search over poses — it never touches the physics engine (`docs/optimize-decision.md`). |
| `gradiance-physics` | Sync systems derive colliders/joints from authored state; `queries` is the shared read cut-point. (avian is used directly — `docs/physics-deadapter-decision.md`.) |
| `gradiance-signal` | The dataflow: bus, bindings, params, computed signals, and the lowering to `gradiance-kernel`. |
| `gradiance-command` | The choke point: `intent` (typed messages), `dispatch` (the sole mutator), `CommandStack`. |
| `gradiance-persist` | RON save/load of materialized authored state. |
| `gradiance-interaction` | Picking, selection, camera, `tools/` (the tool facade — see §6), and the ECS half of packing. |
| `gradiance-render` | Derived meshes/materials, toon look, grid, joint gizmos, debug overlays. |
| `gradiance-script` | The DSL: `bridge` (the one ECS/steel seam, `run_scripts`), `registry` (operation catalog), `reflect_bridge`. **Only** steel import. |
| `gradiance-ui` | **Only** egui import. Docks, toolbar, inspectors, context menu, settings, console, node canvas, plotter, curve editor. Thin projections + intents. |

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

- **A new edit / command:** add one row to the `command_intents!` table in
  `command::dispatch` (it registers the message + reflected type and dispatches
  it), plus the intent and command types. Mutate only authored components;
  provide `undo`.
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
lower to the compiled, allocation-free numeric **kernel** (`gradiance-kernel`,
Tier B) that runs over query/buffer columns in one system. The package graph
states this: `signal` depends on `kernel`, not on `script`. Note the rule
applies to *shapes* too — an authored response curve is sampled into a `Lut` at
compile time, so the frame loop does a lerp, not a segment search. Bulk/particle
updates are *derived* — never commands, never undo-recorded, never persisted.

---

## 7. The scripting / DSL north star (accepted design)

Ratified in `docs/script-lisp-decision.md`: a **Lisp/DSL over a governed,
homoiconic operation registry** as the tool's control plane. Programmability is
not one milestone — it **accretes through** the substrate so we script real
features, not placeholders. Both linchpin spikes have passed:

- **Perf spike:** `gradiance-kernel` — numeric DSL → flat allocation-free tape,
  VM-free hot-path eval (~27.7 M evals/s debug at particle scale).
- **Reflect↔steel spike:** `script::reflect_bridge` — one generic converter
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

**Landed:** full command/undo core · the workspace split (one package per layer,
DAG asserted as data) · typed SI units · SDF geometry + cut/merge · draw tools
(box/circle/polygon) · manip tools (select/drag) on the `ManipTool` facade ·
joints (hinge/weld/slider) + motors · **strut** spring/damper
(`JointKind::Spring`, mass-based default stiffness, coil gizmo, inspector) ·
contact-point/force debug overlay (reads avian `ContactGraph`) · **signal
dataflow P2** (bus, bindings, params, computed signals lowered to the kernel) ·
node canvas (`egui-snarl`) · **script P1**: operation registry +
Edit/Config/Query/EditorState verbs + REPL console + `--script foo.scm` loader +
data-driven context-menu actions · **layout optimizer** (`gradiance-optimize`,
Shelf/Descent/Naive behind one `Solver` trait) · **array patterns**
(Ctrl-drag a handle → linear or grid, flush-pitch spacing) · the `egui_tiles`
dock shell + menu bar · **plotter on `egui_plot`** (real time axis, zoom/pan,
legend, cursor) · the **curve editor** and `BlockOp::Curve`.

**Sequencing (`docs/roadmap.md` §"Sequencing after M17.1"):** substrate first,
then script it. Order: (1) interactions & joints/constraints + their UI →
(2) tracers/plotters/probes → (3) script the above (P2 dataflow) → P3 symbolic.

**Queued / open to brainstorm:**

- **Curve editor beyond signals** — nonlinear strut stiffness/damping. The
  widget exists (`ui::curve`); what is missing is a joint-side authored value
  that is a *function* rather than a scalar.
- More plotter signals (contact force), **pinnable multi-body probes**, and the
  script-driven `(measure …)` data-out seam.
- **UI polish pass** — largely landed; `docs/ui-design.md` is the record.
  A font/glyph vocabulary (no tofu, enforced by a source scanner), an
  image-icon registry, a shared widget vocabulary, layout-preserving docks with
  closable tabs, the `egui_plot` plotter and curve editor, a top-down depth
  plan view with scene-wide draggable depth lines, and the Signals pane folded
  into the node canvas as a side list. Remaining: node-canvas comment blocks
  and **persisted block positions** (an open classification question — document
  content or workstation layout?).
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
list), `docs/bevy19-notes.md` (version gotchas), `docs/ui-design.md` (the
chrome design system — glyphs, widget vocabulary, seam rules, keybindings),
`docs/feature-feedback.md` (user feedback log).
