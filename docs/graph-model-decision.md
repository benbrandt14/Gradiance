# One graph model — decision and phased plan

**Status: decided (session best estimate, 2026-11-20); the owner may
reverse it.** Decided under the owner ruling of 2026-11-17 ("Leave non
interactive changes to the thread here. Go with your best estimates"):
an architecture option is made by the session and recorded, not asked. The
record is the `graph-model` entry in `docs/review/archive.ron`. This is
stage G-0 of the next planning stage (`docs/state.md`, roadmap item 12).

The phases below become briefs in the order given. G-1 is built (in
review; `docs/briefs/G-1-graph-ir.md`); brief G-2 is written
(`docs/briefs/G-2-graph-scene-content.md`). Each later brief is written at
its predecessor's handoff.

## The question

`docs/unified-surface-plan.md` § 5 says: "The signal graph and the geometry
graph should become **one** graph model … that is the decision to make".
The alignment rule there is **one IR, three views**: a typed DAG of calls
(the IR), with Lisp text, the node canvas and the inspector/tools as views,
and evaluation in compiled Rust (never Steel in the frame loop).

So the question is: what is that one IR, what does it hold, and how much
of the document does it own?

Bounds (owner rulings, `docs/state.md`):

- Persistence stays **single-format RON of materialized authored state**.
- The **two-tier rule**: Steel never runs in the frame loop.
- **Format may change freely**: bump `FORMAT_VERSION`, no migrations.
- **Graphite is the capability goal, MVP first. Port, do not depend.**
- **Keys are out of scope.**
- **Node-canvas positions wait for G-0**, because they become document
  content if the graph becomes the document.

## What the code has today (main `523ed82`)

There are not two graphs today. There is one graph and one tree.

**The signal graph is a graph, wired by name.**

- Authored data: three config-seam resources in
  `crates/gradiance-domain/src/signal.rs` (1424 lines): `SignalBindings`
  (source → map → curve → gradient → sink), `SignalParams` (`defparam`
  knobs) and `ComputedSignals` (`defsignal`, each a `SignalExpr` tree and
  an optional canvas `BlockOp`).
- Node kinds: `BlockOp` has 12 variants (Time, Constant, Oscillator, Gain,
  Sum, Product, Sub, Abs, Min, Max, Curve, Key); a console `defsignal` is
  an RPN-parsed `SignalExpr` with no block;
  `SignalSource` has 14 reads, `SignalSink` 8 writes.
- **Edges are bus names (`String`), not ids.** A block operand is
  `Option<String>`; a binding's `Named(String)` source reads the bus.
  Body, joint and node references are `StableId`. Renaming a param breaks
  every wire that names it.
- Persistence: the three resources are fields of `EnvironmentRecord` and
  are listed in `scene_content!` (`crates/gradiance-scene/src/records.rs`
  lines 458–467; 3 of its 8 fields). A settled edit is one `settings` undo
  step (U-2c).
- Evaluation: `gradiance-signal` (`lib.rs` 707 lines, `compile.rs` 101)
  lowers each `SignalExpr` to a `gradiance_kernel::Kernel` once
  (`recompile_signals`), then `evaluate_computed` runs the tapes per frame.
  Cycles read last frame's bus value. This is the two-tier rule, built.
- The canvas: `crates/gradiance-ui/src/node_graph.rs` (2108 lines, on
  `egui-snarl` 0.12). `reconcile` rebuilds the snarl graph from the
  resources each frame, keyed by `GraphKey` (Body, Joint, Node by id;
  Param, Computed by **name**; Scope; Comment). Positions, the "added
  bodies" set, body names and comments are view state in the `NodeGraph`
  resource, never saved.
- The canvas **writes the resources directly** (`GraphParams` holds
  `ResMut` of all three; `joint_inspector.rs:44` holds one too). There is
  no `signal.bind` operation (retained item, N-1 Outcome). Params, computed
  signals and keys have operations (`signal.param`, `signal.computed`,
  `signal.key`, config seam, in `crates/gradiance-script/src/ops.rs`).
  So the canvas and Lisp do not share one surface for wires.
- Reach: 15 source files and 12 integration-test files name the three
  resources.

**The geometry "graph" is a tree per body, and it is materialized.**

- `ShapeDef` (`crates/gradiance-geometry/src/shape.rs`): 5 leaves (Box,
  Circle, Polygon, Capsule, HalfPlane) and 2 composite nodes (`Csg` with 5
  `CsgOp`s, `Placed`). Children are `Box<ShapeDef>`: no sharing, no
  parameters, no references to anything.
- Booleans are **destructive commands**: `MergeCommand` folds operand
  bodies into the host's tree and despawns them (`merge_cmd.rs`). A split
  result bakes to polygon leaves; a tree deeper than `MAX_CSG_DEPTH` (8)
  bakes too. No history lives in the tree; undo restores records.
- `ShapeDef` is authored and saved in `BodyRecord.shape`. 70 source files
  and 28 test files name it. `geometry::polygonize` is the one
  discretization point.
- No operation takes or returns a shape value. Shapes are built by
  `body.spawn_*` (numbers) and changed by body-index booleans.

**The operation registry has no node concept yet.**

- `OpMeta` (`crates/gradiance-op/src/registry.rs:29`) has id, Lisp name,
  doc, seam, menu, chord, undo label and example. There is **no `node`
  flag**, and outputs are untyped (`serde_json::Value` via
  `OpOutput::returning`). § 5 asked for both; neither was built.
- 150 operations (`docs/ops.md`): 46 edit, 49 query, 18 config, 44
  editor (rows counted by seam in the generated table, approximate).

What this means for the options: "unify two graphs" really means "give the
one graph typed, id-based edges, and give geometry a parametric form that
the same graph can carry". The shape tree is already the right *compiled*
form: it is to geometry what `Kernel` is to signals.

## Options

### (a) Keep two graphs

The signal graph stays as it is. Geometry stays a per-body tree. Add only
shared node metadata (Graphite's `category`, `name`, `default`, `hard`) so
both look alike on the canvas.

- **Cost:** smallest. No format change.
- **Evidence against:** the drift problem stays. Edges stay names, the
  canvas keeps its `ResMut` side door, and parametric geometry (the
  Graphite capability) needs a second graph later with its own canvas,
  save path, undo path and Lisp view. That is the drift § 0 item 9 forbids
  ("one integrated system that … cannot become misaligned").
- **3D:** a third graph for 3D geometry, later.

### (b) One IR; signals and shapes are two node families

One typed graph IR: nodes with `StableId`s, typed ports, edges by
`(node id, port)`. Each node kind declares its **family** and **rate**:

- **signal family, frame rate:** today's blocks, params, keys, sensor
  reads, sinks. Lowers to `SignalExpr`, then to the kernel, as now.
- **shape family, edit rate:** shape leaves, `Csg`, `Placed`. Lowers to
  a `ShapeDef`, once per settled edit. `ShapeDef` stays the one geometry
  type every consumer reads.
- **Rate rule:** an edit-rate input never takes a frame-rate output (a
  typed error at validation). A param (edit-rate scalar) may feed both
  families. So a slider can size a box and scale a gain, but a body's
  speed cannot resize a collider every frame.

Bodies, joints and behaviour nodes stay authored entities. On the graph
they are **reference nodes** (`StableId`): their sensor ports are outputs,
their actuator ports and (later) their shape are inputs. The graph is one
scene-content resource, saved with the scene and undone like the signal
resources now. Graph *edits* are operations; graph *nodes* are not.

- **Cost:** medium, and it comes in brief-sized steps (plan below): a new
  pure crate and one DAG row; two format bumps (graph replaces the signal
  resources; a body may name a shape node); the canvas adapter rewritten
  around the graph (it gets simpler: the graph is the source, `reconcile`
  stops keying by name); the port catalog moves out of `gradiance-ui`.
- **Unlocks:** id-based wires that survive renames; one edit surface for
  the canvas, Lisp, remote and MCP (`graph.*` operations); parametric
  shapes driven by params (the Graphite MVP); node positions saved as
  document content; one node catalog projected like `docs/ops.md`.
- **Invariants:** no new mutation path (the graph stays on the config
  seam; the canvas moves from `ResMut` to operations). Invariant 5 gains
  one case: a body that names a shape node has a **derived** `ShapeDef`.

### (c) The graph becomes the whole document (Graphite's model)

Every body, joint and setting is the output of a graph node. The save file
is the graph. The scene is its evaluation.

- **Cost:** large, and it collides with three rulings and one physical
  fact:
  - The save is no longer materialized authored state; it is a program
    (`docs/script-lisp-decision.md` § The four forks rejected this:
    "exactly one save format … RON of materialized state").
  - **Bodies move.** Physics changes poses every step, and a played scene
    is saved as it stands. A graph-generated body needs a "rest pose vs
    live pose" split that Graphite never needs (it has no simulation).
  - All 46 edit operations and every command become graph edits; the
    undo path (records, `CommandStack`, the 55-row undo matrix of 6-1)
    is rebuilt.
  - Destructive booleans, cuts and the sketch tool become node inserts;
    `MAX_CSG_DEPTH` baking has no place.
- **Unlocks:** full non-destructive history, procedural scatter and
  instancing of bodies.
- **3D:** every engine or plane change becomes a change to how the whole
  document evaluates. More expensive, not less.

## Decision

**(b): one IR, two node families, two rates. Not (a), not (c).**

Why:

- It is the smallest change that makes § 0 item 9 true for the graph:
  the canvas, Lisp, remote, MCP and CLI edit one graph through one set of
  operations, and conformance tests can check it, as they check the
  operation registry now.
- It reuses what works. The kernel lowering, the bus, `ShapeDef`,
  `polygonize`, records, the settings-watcher undo and every shape
  consumer stay. The new parts are the IR, its validation and the two
  lowerings, all pure.
- It gives the Graphite MVP (a parametric shape driven by a param) without
  the cost of (c). (c) stays reachable later as a family of
  "instancer" nodes, if the owner wants it, but this plan does not build
  toward it.
- The rate rule is the two-tier rule applied to the graph: frame-rate
  nodes lower to the kernel; edit-rate nodes lower to geometry calls on a
  settled edit. Steel never evaluates a node.

### Design points fixed by this decision

1. **Crate.** A new pure crate `gradiance-graph` (`graph → core, domain`;
   bevy only for `Reflect`/`Resource`/serde derives, like `geometry`). It
   holds the IR, the node catalog, validation and the two lowerings. No
   systems, no queries. `signal`, `scene`, `command` and `ui` depend on it
   from G-2 on.
2. **Identity.** Every node has a `StableId`. An edge is
   `(node StableId, output port index)`. A bus name is a node's published
   label, not an edge (invariant 3 now holds for wires too).
3. **Node catalog, Graphite vocabulary.** Each node kind has a
   `NodeMeta`: `id`, `category`, `name`, doc, input ports (`name`, type,
   `default`, `hard` range) and output ports. These are Graphite's
   `#[node_macro::node]` field names as `docs/unified-surface-plan.md`
   § 5–6 records them, so a later port maps field by field. A generated
   `docs/nodes.md` follows (G-2).
4. **Nodes are not operations.** The § 5 idea of a `node` flag on
   `OpMeta` is dropped: an operation lowers to messages for a seam, a node
   is a pure function of its inputs. The link is the edit surface:
   `graph.add`, `graph.connect`, `graph.set`, `graph.remove` (and later
   `signal.bind`/`signal.unbind` as thin aliases) are operations on the
   config seam.
5. **Persistence.** One `Graph` resource, classified as scene content in
   `scene_content!`, replaces `SignalParams`, `ComputedSignals` and then
   `SignalBindings`. Node positions are node fields: saved and undoable
   (a settled drag is one `settings` step, like a slider drag).
6. **Shapes stay materialized by default.** A body keeps an authored
   `ShapeDef` unless it names a shape node. Then its `ShapeDef` is derived
   from the graph and not saved. A destructive edit (cut, boolean, scale,
   mirror) on such a body first **bakes** it (authored `ShapeDef` = the
   current value, link removed) in the same undo step.

## What it costs

- One new crate and one reviewed DAG row in `tests/boundaries.rs` and the
  `CLAUDE.md` diagram.
- Two `FORMAT_VERSION` bumps (G-2, G-4); goldens regenerate.
- The canvas adapter (`node_graph.rs`, 2108 lines) is rewritten in two
  briefs. Its `GraphKey` name keys go; its `NodeGraph` view state shrinks
  to pan/zoom and the open flag.
- The port catalog (`ui::ports`, 256 lines) moves down into
  `gradiance-graph` (the orchestration rule: logic a script needs does not
  live in UI).
- The settings watcher compares the graph by value every frame. The cost
  is `O(nodes + edges)` per frame; it is the same pattern as today's
  three resources. A graph of thousands of nodes would need a version
  counter instead (not now).
- Invariant 5 gains the derived-shape case and the bake rule (point 6).

## What it forecloses

- **Graph-as-document (c)** in this plan. Bodies, joints and settings are
  not graph outputs. Procedural scatter or instancing of *bodies* needs a
  new decision.
- **Operations as nodes** (the § 5 `node` flag). An operation never runs
  inside graph evaluation.
- **Frame-rate geometry.** A shape input never takes a per-frame signal,
  so there are no animated colliders. A later "step-rate" family would be
  its own decision with its own cost (one `polygonize` and collider
  rebuild per step).
- **Name-addressed wires.** A script that wants a value on the graph
  publishes it with `signal-set`, and a `bus-read(name)` node reads it.
  That node is the only name-addressed input.

## How it keeps 3D-0 cheaper

The frames plan (`docs/frames-decision.md`, F1–F5) and this plan touch
different files and different seams:

- **Graph values are plane-local.** Scalars and `ShapeDef` carry no world
  axis, like intents and records in the frames decision ("Stay": intents,
  ops and records carry plane-local SI `Vec2`). F2's routing of the 51
  sites never enters the graph crate. The rule is written into the IR
  docs in G-1 so no node adds a world axis.
- **Two sensor reads are world-axis today:** `Height` (world y) and
  `PosX` (world x). They are rows of the quantity catalog, read through
  `physics::queries`. When F4 gives bodies a plane, they become
  plane-local reads in the catalog row, one place. The graph does not
  change.
- **Port types are an open enum.** F4 planes and F5 datums are authored
  entities with `StableId`s, so they are reference nodes like bodies. A
  `Frame` port type (a `PlaneFrame` value) lets a sketch plane feed a
  shape node: CAD's "sketch on a datum" is one port type, not a second
  graph.
- **3D geometry is a third family, not a third graph.** An extrusion or a
  3D SDF family lowers to its own compiled form on the same IR, with the
  same catalog, canvas, operations and Lisp view.
- **No engine coupling.** The graph never names avian. Frame-rate sinks
  write derived components (`SignalDrive`) that physics reads, as now, so
  the engine swap stays behind `physics::queries`.
- Under (a), 3D adds a third graph; under (c), the engine swap changes how
  the whole document evaluates. Only (b) makes it cheaper.

Order: G-1 and G-2 do not touch any file F1–F3 touch (`core::units`, the
51 sites, `GridSettings`). They may run in either order with F1–F3.

## Phased plan

Each phase is one brief, one worker, one PR, and leaves fmt, clippy, doc
and tests green. Each names the test that proves it.

| Phase | What lands | The test that proves it | Format |
|---|---|---|---|
| **G-0** | This decision; `graph-model` in `archive.ron`; brief G-1. | `cargo test --test it -- review` parses the archive. | — |
| **G-1** | `gradiance-graph`: the IR, `NodeMeta` catalog, validation (port types, rates, cycles), the signal lowering (graph → `SignalExpr` per published node) and the shape lowering (graph → `ShapeDef`); a pure converter from today's `SignalParams` + `ComputedSignals` to a graph. No ECS, no UI, no format change. Brief: `docs/briefs/G-1-graph-ir.md`. | Every `BlockOp` variant and a set of RPN `defsignal` expressions, converted and lowered, gives a `SignalExpr` equal to today's; a shape graph lowers to an equal `ShapeDef` for every leaf, `Csg` and `Placed`; a frame-rate output into a shape input is a typed error. `cargo test -p gradiance-graph`. | none |
| **G-2** | The graph is scene content: a `Graph` resource replaces `SignalParams` and `ComputedSignals` in `scene_content!`; `recompile_signals` reads it; operands are edges by id; `signal.param`, `signal.computed`, `signal.key` lower to graph edits; `graph.*` operations; param and computed blocks keep their saved positions; generated `docs/nodes.md`. | Save, load and undo of a wired block graph with positions (exact `scene.dump`); renaming a param keeps its wires (it broke them before); the undo matrix and conformance tests cover the new operations with no new list. | bump to 9 |
| **G-3** | Bindings become edges: body, joint and behaviour-node reference nodes; map, curve and gradient become a colour node; `SignalBindings` goes; the port catalog moves out of `ui`; the canvas edits through operations only (`ResMut<Graph>` in `gradiance-ui` fails a scanner test); `signal.bind`/`signal.unbind`; the added-bodies set and body names are saved. | A wire made from Lisp, from the canvas invoke path and from remote gives equal graphs; the side-door scanner finds no direct graph write in `ui`; every `tests/it/signals.rs`-family test still passes. | bump |
| **G-4** | Shape family live: a body may name a shape node; its `ShapeDef` is derived on a settled graph edit (change-detected, edit rate); destructive edits bake first in the same step; shape nodes on the canvas; a shape graph deeper than `MAX_CSG_DEPTH` is refused at validation. | A param drives a box's width and the collider follows after one settled edit; a cut on a graph-driven body bakes and is one undo step, and undo gives the link back; a played-then-saved scene reloads with the same derived shape. | bump |
| **G-5** | The Lisp view: graph ↔ s-expression text (print and read), `(graph-print)` and a `(graph …)` form that lowers to `graph.*` operations. | Property test: print then read gives an equal graph for generated graphs; a script that builds a graph and the canvas that builds the same graph give equal `scene.dump`s. | none |
| **G-6** | Graphite review: read Graphite's `node-graph` (`node-macro`, `graph-craft`) at a recorded revision; port the declaration shape (a derive or attribute that writes `NodeMeta`) if it fits; update `docs/upstream-watch.md` and `vendor/UPSTREAM.toml`. | Every catalog node declared through the ported form; `docs/nodes.md` unchanged by the switch. | none |

G-1 to G-3 complete the "one IR" for what exists. G-4 is the first
Graphite capability. G-5 completes the three views. G-6 may move earlier
if a session can read Graphite's source before G-2.

## Review questions

None added. Every choice here is an architecture option that no scene can
show. One later choice may become interactive: in G-3, how the colour
mapping looks on the canvas (a separate colour node, or map and gradient
on the wire as today's body footer shows them). G-3 asks it in review
mode if both forms can be shown in a scene; otherwise it decides.

## What I did not check

- **Graphite's source.** Not fetched (this session's rule). Its vocabulary
  here comes only from `docs/unified-surface-plan.md` § 5–6. G-6 reads
  the source.
- **`egui-snarl` 0.12 position API.** That the canvas can seed and read
  back each node's position from external data (it inserts at a position
  today; reading it back for save is unverified).
- **Insert and paste (R-3).** Whether `scene.insert` and `edit.paste`
  carry environment content today. Graph nodes need fresh ids on insert;
  G-2 checks it.
- **Duplicate.** `remap_binding` copies a body's bindings on duplicate;
  the reference-node equivalent is G-3's to design.
- **Kernel limits.** `MAX_VARS` is 16 inputs per compiled expression.
  G-1 keeps one compiled expression per published node, as today, so the
  limit does not change; a whole-subgraph fusion would hit it.
- **Watcher cost.** The per-frame by-value compare of a large graph was
  not measured.
- **Counts.** Operation counts by seam come from the generated table's
  rows and are approximate. No code was compiled for this plan.
