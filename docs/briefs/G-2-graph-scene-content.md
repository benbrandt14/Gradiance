# The graph becomes scene content
Phase G (one graph model) · `docs/graph-model-decision.md` § Phased plan,
row G-2; `docs/state.md` roadmap item 12 · status: ready

Written by the G-1 worker at handoff, 2026-11-20, on the G-1 branch
(`claude/project-thread-tw1mvl-g1`, stacked on G-0 #296). Code change; one
worker, one PR. Start from main once G-0 and G-1 are merged.

## Goal

One `gradiance_graph::Graph` resource replaces `SignalParams` and
`ComputedSignals` as scene content. Operands become edges by id, so
renaming a param keeps its wires. Node positions are saved and undoable.

1. **Scene content.** `Graph` is a field of `EnvironmentRecord` and of
   `scene_content!` (`crates/gradiance-scene/src/records.rs`, the list at
   line 458) in place of `params` and `computed`. `SignalBindings` stays
   (G-3). `FORMAT_VERSION` 8 → 9 (`crates/gradiance-scene/src/lib.rs:59`);
   no migration (owner ruling: format may change freely); goldens
   regenerate.
2. **Runtime.** `recompile_signals` (`crates/gradiance-signal/src/lib.rs`
   line 215) reads `Res<Graph>` through `gradiance_graph::lower_signals`
   and compiles each `(label, SignalExpr)` as today; params publish their
   value under their label each frame, as today. A graph that fails
   validation compiles nothing it cannot lower and lists the error where
   `CompiledSignals::errors` does now.
3. **Operations.** `signal.param`, `signal.computed`, `signal.key`
   (`crates/gradiance-script/src/ops.rs` from line 300) lower to graph
   edits (same Lisp, same arguments: re-declaring a name updates that
   node). New config-seam operations `graph.add`, `graph.connect`,
   `graph.set` (a port constant, a label or a position), `graph.remove`,
   addressing nodes by `StableId` or by label. They go into the undo
   matrix (`tests/it/undo_matrix.rs`) and the conformance tests through
   their examples, with no new list.
4. **Canvas.** `crates/gradiance-ui/src/node_graph.rs` reads params and
   computed blocks from the graph: `GraphKey::Param` and
   `GraphKey::Computed` key by node id, not name; a block's position comes
   from `Node::pos` and a settled drag writes it back (one `settings`
   step). Wiring a block still writes the resource directly in this brief
   (the `ResMut` side door goes in G-3); it now writes `Input::Wire`.
5. **Catalog doc.** A generated `docs/nodes.md` from
   `gradiance_graph::CATALOG`, checked like `docs/ops.md`
   (`tests/it/op_conformance.rs` line 830: a test that fails when stale and
   rewrites with an env var).

## Read first

1. `docs/graph-model-decision.md` § Design points 4–5.
2. `docs/briefs/G-1-graph-ir.md` § Outcome (the API and its decisions)
   and `crates/gradiance-graph/src/lib.rs`.
3. `crates/gradiance-scene/src/records.rs` (`scene_content!`,
   `EnvironmentRecord`), `crates/gradiance-command/src/lib.rs` near line
   376 (`commit_settings_edits`).
4. `crates/gradiance-signal/src/lib.rs` (`recompile_signals`,
   `evaluate_computed`, `publish_sensor_refs`: it reads the names the
   compiled signals use).
5. `crates/gradiance-script/src/ops.rs` (the three `signal.*` ops) and the
   `EditorRequest::Param` / `Computed` handlers they message.
6. `tests/boundaries.rs` DAG table: `scene`, `signal`, `command`, `script`
   and `ui` gain the `graph` edge (one reviewed row change each).

## Facts (G-1 worker, 2026-11-20)

- `from_signals(&SignalParams, &ComputedSignals, ids)` converts today's
  resources; every current block and RPN signal lowers back `==` to its
  `ComputedSignal.expr` (tests in `crates/gradiance-graph/src/tests.rs`).
  Use it in tests to build graphs from today's fixtures.
- `lower_signals` returns labelled signal nodes except params, in
  evaluation order. `Order::feedback` lists the edges broken to evaluate
  a cycle (they read last frame's bus value, as today).
- Node ids in the catalog are `signal::gain`, `shape::box` (`::`), so they
  never clash with operation ids such as `signal.param`.
- A block's numbers (gain `k`, oscillator `amp`/`freq`, constant `value`,
  the curve) are input ports with constants (`graph-block-constants` in
  `archive.ron`). The canvas edits them as port constants.
- `Graph` derives `Resource`, `Reflect`, serde, `PartialEq`, `Default`;
  nothing registers it yet. `StableId` has no `Ord`: the graph sorts by
  the inner `Uuid`.
- 17 files in `crates/*/src` and `tests/it` name `SignalParams` or
  `ComputedSignals` (`grep -rl`).

## Approach

- Keep `SignalParams`/`ComputedSignals` types only if a test fixture needs
  them as converter input; remove them as resources. Prefer one PR that
  moves every reader to the graph over a mirror kept in sync.
- New ids for new nodes come from `StableId::new()` in the operation that
  creates them (the snapshot rule: a script run reads pre-run state, so a
  `defparam` then a `defsignal` naming it in one run must still wire; check
  how names resolve within one run and pin it with a test).
- `scene.insert` / `edit.paste` (R-3) give fresh ids to everything they
  insert: graph nodes need the same remap (the G-0 "not checked" item).
- The settings watcher compares the graph by value each frame, like the
  three resources now.

## Out of scope

- Bindings, reference nodes, the colour node, the canvas side door,
  `signal.bind` (G-3). Shape nodes on bodies (G-4). Lisp printing of
  graphs (G-5). Graphite source (G-6).

## Done when

- Save, load and undo of a wired block graph with positions give exact
  `scene.dump`s; renaming a param keeps its wires (a test that fails on
  main, where the rename breaks them).
- Every `tests/it/signals.rs`-family test passes; the undo matrix and the
  conformance tests cover the `graph.*` operations from their examples.
- `docs/nodes.md` is generated and checked; `docs/ops.md` regenerated.
- fmt, clippy, doc, `cargo test --test boundaries`, the unit tests of every
  touched crate and the touched `tests/it` modules are green;
  `FORMAT_VERSION` is 9 and the goldens are regenerated.
- `CLAUDE.md` (invariant 4's scene-content list), `docs/state.md` and this
  Outcome are updated; decisions in `archive.ron`.

## Outcome

*(filled by the worker at handoff)*
