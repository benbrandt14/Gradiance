# The graph IR crate
Phase G (one graph model) · `docs/graph-model-decision.md` § Phased plan,
row G-1; `docs/state.md` roadmap item 12 ("**G-0** one graph model") ·
status: ready

Written by the G-0 planning session, 2026-11-20, on main `523ed82`.
Code change; one worker, one PR.

## Goal

A new pure crate `gradiance-graph` holds the one graph IR that the signal
graph and (later) parametric shapes share. It has:

1. **The IR:** nodes with a `StableId`, a node kind, input bindings and a
   saved position; an edge is `(source node StableId, output port index)`;
   an unconnected input holds a constant or reads 0.
2. **The node catalog:** one `NodeMeta` per node kind, in Graphite's
   vocabulary: `id`, `category`, `name`, doc, input ports (`name`, port
   type, `default`, `hard` range) and output ports. Each kind declares its
   **family** (signal, shape) and **rate** (frame, edit).
3. **Validation:** port types match, an edit-rate input never takes a
   frame-rate output, edges name existing nodes and ports, and frame-rate
   cycles are found and reported (they read last frame's value, as
   today). A shape graph deeper than `MAX_CSG_DEPTH` is refused.
4. **Two lowerings:** the signal family lowers to one
   `gradiance_domain::signal::SignalExpr` per published node (the input
   `signal::compile` already takes); the shape family lowers to a
   `ShapeDef`.
5. **A converter** from today's `SignalParams` + `ComputedSignals` to a
   graph, with names resolved to edges by id.

Nothing in the app uses the crate yet. No ECS, no UI, no format change.
G-2 wires it in.

## Read first

1. `docs/graph-model-decision.md`: § Decision, § Design points 1–6, and
   § How it keeps 3D-0 cheaper (the plane-local rule).
2. `crates/gradiance-domain/src/signal.rs`: `SignalExpr` (line 655),
   `ComputedSignal` (789), `BlockOp` (825) with `to_expr`,
   `input_slots`, `set_input`, `label` (from 931), `SignalParam` (619).
3. `crates/gradiance-signal/src/compile.rs`: how a `SignalExpr`'s named
   inputs become kernel slots. The new lowering ends where this begins.
4. `crates/gradiance-geometry/src/shape.rs`: `ShapeDef`, `CsgOp`,
   `MAX_CSG_DEPTH`, `validate`.
5. `tests/boundaries.rs` lines 188–290 (the crate DAG table) and the
   `CLAUDE.md` layer diagram.

## Facts (G-0 session, 2026-11-20, on `523ed82`)

- The signal graph's edges are names today: a `BlockOp` operand is
  `Option<String>`; a `SignalExpr::Input(String)` reads the bus. Producers
  of a name: a param (`SignalParam.name`), a computed signal
  (`ComputedSignal.name`). Every other input name is **external**: `"t"`
  (`signal::TIME_INPUT`), sensor reads `"<tag>@<uuid>"`
  (`SignalSource::bus_name`), key inputs `"key@<key>"`
  (`domain::signal::key_input_name`), laser outputs `<name>` and
  `<name>.distance`, and script-published names (`signal-set`).
- `ComputedSignal.expr` is always the compiled form; for a canvas block it
  is derived from `block` by `BlockOp::to_expr`. A console `defsignal` has
  `block: None` and an RPN-parsed `expr`.
- `BlockOp` has 12 variants: Time, Constant, Oscillator, Gain, Sum,
  Product, Sub, Abs, Min, Max, Curve, Key. `SignalExpr` has 13.
- `ShapeDef` has 5 leaves (Box, Circle, Polygon, Capsule, HalfPlane) and
  2 composites (`Csg { op, lhs, rhs }`, `Placed { pos, rot, shape }`).
  `CsgOp` has 5 (Union, Subtract, Intersect, Xor, SmoothUnion).
- `gradiance-domain` re-exports the shape module (`domain::shape`), so the
  new crate needs only `core` and `domain` as `gradiance-*` edges.
- Graphite's metadata names (`category`, `name`, `default`, `hard`) come
  from `docs/unified-surface-plan.md` § 5–6. Graphite's source was **not**
  read (unverified); do not fetch it for this brief (G-6 does).

## Approach

- **Crate:** `crates/gradiance-graph`, `[lints] workspace = true`,
  dependencies `gradiance-core`, `gradiance-domain`, `bevy` (only for
  `Reflect` and `Resource` derives), `serde`, `thiserror`; dev:
  `proptest` if a property test helps. Add the DAG row
  `("graph", &["core", "domain"])` to `tests/boundaries.rs` with a
  comment naming this brief, and the line `graph → core, domain` to the
  `CLAUDE.md` diagram with one paragraph like the `optimize` one. Add the
  root re-export `gradiance::graph` only if a test in `tests/it` needs it
  (it should not).
- **Types (names are the worker's to choose; shapes are fixed):**
  - `Graph { nodes: Vec<Node> }` sorted by id, `Serialize`,
    `Deserialize`, `Reflect`, `PartialEq`, `Default`; derive `Resource`
    now so G-2 can insert it, but register nothing.
  - `Node { id: StableId, kind: NodeKind, inputs: Vec<Input>, label:
    Option<String>, pos: Vec2 }`. `label` is the published bus name of a
    signal node. `pos` is canvas position (saved from G-2 on).
  - `Input = Const(Value) | Wire { node: StableId, port: u8 } | External(String)`.
    `External` is the one name-addressed input (decision § What it
    forecloses), for bus values the graph does not produce.
  - `PortType`: `Scalar`, `Shape`, `Curve` (a `Curve` value), plus what
    the shape leaves need (`Vec2`, `Points`). Keep it an enum that G-4
    and F4 can extend (`Frame` later). `Value` holds a constant of each.
  - `Rate`: `Frame`, `Edit`. A param is an edit-rate scalar that also
    publishes each frame (it may feed both families).
  - `NodeKind`: one variant per `BlockOp` variant, `Param { value, min,
    max }`, `Expr(SignalExpr)` for a console `defsignal` (its named inputs
    become ports in first-appearance order, the order `SignalExpr::inputs`
    returns), and the shape family: one per `ShapeDef` leaf, `Csg(CsgOp)`,
    `Placed`. No reference nodes (bodies, joints) yet: G-3.
- **Catalog:** `fn meta(kind) -> &'static NodeMeta` or a static table;
  a test asserts every `NodeKind` has a row and every row's ports match
  what lowering reads.
- **Validation:** `Graph::validate() -> Result<Order, GraphError>`
  (`thiserror`), where `Order` is a deterministic evaluation order
  (topological; ties by id; a frame-rate cycle is broken at the edge into
  the lowest id and reported in `Order`, not as an error). Errors: unknown
  node or port, type mismatch, `RateMismatch` (frame-rate output into an
  edit-rate input), an edit-rate cycle, duplicate label, a shape deeper
  than `MAX_CSG_DEPTH`.
- **Signal lowering:** `lower_signal(&Graph, id) -> Result<SignalExpr, _>`
  for every node with a label. A `Wire` to a labelled node lowers to
  `SignalExpr::Input(label)`, so `signal::compile` and the bus stay as
  they are (one compiled expression per published node, as today; this
  keeps the kernel's 16-input limit where it is). `External(name)` lowers
  to `Input(name)`.
- **Shape lowering:** `lower_shape(&Graph, id) -> Result<ShapeDef, _>`
  builds the tree from the shape subgraph rooted at `id` and runs
  `ShapeDef::validate` on the result. A node used twice is copied (the
  tree has no sharing); that is correct and costs memory only.
- **Converter:** `from_signals(&SignalParams, &ComputedSignals, ids)` →
  `Graph`, where `ids` supplies new `StableId`s (a closure or iterator, so
  tests are deterministic; never `StableId::new()` inside). A name that
  matches a param or computed label becomes a `Wire`; any other name
  becomes `External`.
- **Plane-local rule:** the crate docs say every value is plane-local; no
  node kind reads or writes a world axis (decision § How it keeps 3D-0
  cheaper).
- **Invariants:** none touched. The crate has no systems, no queries, no
  intents and no world access.

## Out of scope

- Any ECS wiring: no plugin, no system, no resource inserted, no change to
  `gradiance-signal`, `gradiance-scene`, `scene_content!` or
  `FORMAT_VERSION` (G-2).
- The canvas, `ui::ports`, bindings, sinks, reference nodes (G-3).
- Bodies naming shape nodes, baking (G-4).
- Lisp printing/reading of graphs (G-5); Graphite source and macros (G-6).
- New operations. A `docs/nodes.md` generator (G-2).

Touching any of these: stop and hand back.

## Done when

- `cargo test -p gradiance-graph` passes with at least these tests:
  - for every `BlockOp` variant (operands wired to a param and to an
    external name, and unconnected), `from_signals` then `lower_signal`
    gives a `SignalExpr` equal (`==`) to `ComputedSignal.expr` for the
    same block;
  - for at least five RPN expressions (including a cycle between two
    computed signals and an input named `"t"`), the same equality holds;
  - for every `ShapeDef` leaf, a `Csg` of each `CsgOp`, and a `Placed`,
    `lower_shape` gives a `ShapeDef` equal to the hand-built tree;
  - a frame-rate output (an Oscillator) into a shape input (a Box width)
    is `GraphError::RateMismatch`; a param into the same input lowers;
  - a shape graph deeper than `MAX_CSG_DEPTH` is refused;
  - every `NodeKind` has a catalog row; a graph RON round trip is equal.
- `cargo test --test boundaries` passes with the new DAG row.
- fmt, `cargo clippy --workspace --all-targets -- -D warnings`, and
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib` are
  green. `Cargo.lock` changes only by the new member.
- `CLAUDE.md` diagram, `docs/state.md` and this Outcome are updated.
  Decisions inside this brief go in this Outcome; an architecture choice
  the decision document did not fix goes in `archive.ron` as
  `decision: true`.

## Outcome

*(filled by the worker at handoff)*
