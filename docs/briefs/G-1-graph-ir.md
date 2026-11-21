# The graph IR crate
Phase G (one graph model) · `docs/graph-model-decision.md` § Phased plan,
row G-1; `docs/state.md` roadmap item 12 ("**G-0** one graph model") ·
status: in review

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

Done 2026-11-20 on branch `claude/project-thread-tw1mvl-g1` (stacked on
the G-0 branch, #296).

**What landed.** `crates/gradiance-graph` (pure; `graph → core, domain`;
bevy for `Reflect`/`Resource` only; no systems, no queries, nothing
registered, nothing in the app uses it):

- `ir.rs`: `Graph { nodes }` (sorted by id; `from_nodes`, `node`,
  `insert`, `by_label`; derives `Resource`, `Reflect`, serde), `Node { id,
  kind, inputs, label, pos }`, `Input = Const(Value) | Wire { node, port }
  | External(String)`, `PortType` (Scalar, Vec2, Points, Contours, Shape,
  Curve), `Value` (one constant per port type), `Family`, `Rate`,
  `NodeKind` (12 block kinds, `Key`, `Param { value, min, max }`,
  `Expr(SignalExpr)`, 5 shape leaves, `Csg(CsgOp)`, `Placed`).
- `catalog.rs`: `NodeMeta` (`id`, `category`, `name`, `doc`, `family`,
  `rate`, `inputs`, `outputs`), `PortMeta` (`name`, `ty`, `default`,
  `hard`), the static `CATALOG` (21 rows) and `meta(kind)`.
- `validate.rs`: `Graph::validate() -> Result<Order, GraphError>`: ids
  sorted and unique, every binding (known port, source and output, type,
  rate, `hard` range), unique labels, no cycle of unlabelled signal nodes,
  no edit-rate cycle, no shape deeper than `MAX_CSG_DEPTH`. `Order` is
  Kahn's order with ties by id; a frame-rate cycle is broken at its edges
  into its lowest-id node and listed in `Order::feedback`.
- `lower.rs`: `lower_signal(&Graph, id)`, `lower_signals(&Graph)` (every
  labelled signal node except params, in order), `lower_shape(&Graph, id)`
  (runs `ShapeDef::validate`). Each validates the whole graph first.
- `convert.rs`: `from_signals(&SignalParams, &ComputedSignals, ids)`.
- Crate docs state the plane-local rule.
- DAG row `("graph", &["core", "domain"])` and the serde allowlist row in
  `tests/boundaries.rs`; `CLAUDE.md` diagram line and paragraph.
- `Cargo.lock`: only the new `[[package]]` entry (deps already locked).
  No root re-export (no `tests/it` test needs it).

**Tests** (`cargo test -p gradiance-graph`, 16, 0.01 s): every `BlockOp`
variant with operands wired to a param, to another computed signal, to an
external name and unconnected lowers `==` its `ComputedSignal.expr`; seven
RPN signals (a two-signal cycle, a self-reference, `"t"`, a repeated
name, a block operand) the same, with the feedback edges checked; a
proptest over random expression pairs (cycles, curves) the same; every
shape leaf, a `Csg` of each `CsgOp`, a `Placed` and a node used twice
lower to the hand-built tree; Oscillator → Box width and an external read
→ Box width are `RateMismatch`, a param into it lowers (and feeds a gain
too); 8 placements deep is refused, 7 is not, and a deep constant counts;
each `GraphError` case; every kind has one row, every row's ports are read
by lowering; a RON round trip of a graph with every kind is equal.

**Decisions in this brief:**

- Node ids are `signal::gain`, `shape::box` (`::`, so a node id never
  reads as an operation id: `signal.param` is the operation, `signal::param`
  the node; Graphite also names nodes by path); categories
  "Signal: Sources", "Signal: Math", "Shape: Primitives", "Shape: Combine".
  Placed's ports are `shape, pos, rot` (the primary input first, as
  Graphite does). Polygon holes need a list of point lists: `PortType::
  Contours`.
- `Param` is signal family, edit rate; `Constant` and every other block
  are frame rate (so a Constant block cannot size a box; use a param).
- Every node has one output port today (`port: u8` stays for later).
- An external bus read is frame rate (`RateMismatch` into a shape input).
- `hard` ranges are checked on scalar constants at validation
  (`OutOfRange`); shape lengths have `[MIN_SHAPE_SIZE, f32::MAX]`.
- `lower_signals` leaves params out: a param publishes its value directly,
  as today. `lower_signal` on a param gives `Const(value)`.
- The converter puts every node at the origin; G-2 owns positions.
  `StableId` has no `Ord`, so the graph sorts by the inner `Uuid`.
- `PortMeta::name` is `&'static str` (a static row cannot hold an owned
  name); an `Expr` port's display name is `NodeKind::input_name`.

Architecture choices the decision document did not fix, recorded as
`decision: true` in `archive.ron`: `graph-block-constants` (a block's
numbers are input ports with defaults), `graph-unconnected-shape` (an open
shape port reads the 1 m box), `graph-unlabelled-wires` (a wire to an
unlabelled node inlines; a cycle of unlabelled nodes is refused),
`graph-expr-ports` (a `defsignal` keeps its expression; its names label
its ports), `graph-serde-allowlist` (the graph crate joins the serde
allowlist in `tests/boundaries.rs`).

**Not checked.** Nothing in the app uses the crate, so no `tests/it` run
covers it beyond `-- review`. Lowered expressions were compared with `==`,
not compiled with `gradiance_signal::compile` (it would add an upward
dev edge); equality with today's input makes that redundant. No
performance measurement (validation is `O(n · e)` on a stuck cycle walk;
fine for canvas-sized graphs, see G-2).
