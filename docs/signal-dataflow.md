# Signal dataflow: scaffolding for the node editor

Status: **living document** (2026-07-14). This is the substrate that grows
into the Simulink-style time-series node editor; today it ships as params,
computed signals, and a flat list of bindings with a plain, functional
dock UI (visuals deliberately deferred while the dataflow itself matures).

## The model

The full **sensor → modulator → actuator** chain, all wired over one named
bus:

```
params (defparam)  ──┐                                  ┌─▶ color sink (fill / tracer tint)
sources (reads)  ────┼─▶ SignalBus (name → value) ──────┼─▶ plot
computed (defsignal) ┘        ▲                          └─▶ (future actuators: sim knobs, gizmos)
   kernel-lowered ────────────┘  reads other bus signals
```

- A **binding** is the degenerate two-node graph — one source through a
  map + gradient into one sink:

```
source (read) ──▶ map [in_min, in_max] → t ──▶ gradient(t) ──▶ sink (derived write)
                          └────────────────── value ─────────▶ SignalBus (name → value + history)
```

- **Params** (`defparam name value min max`) are tunable knobs — an
  auto-slider in the Signals dock. Each publishes its value on the bus
  every frame; it is the simplest modulator *input*.
- **Computed signals** (`defsignal name expr`) are the **modulator** tier:
  a named value that is a numeric expression over other bus signals (and
  `t`, elapsed seconds). Authored as a small serializable `SignalExpr`
  tree (RPN in the console — `"t sin amp *"` is `amp · sin(t)`), it is
  **lowered once to the pure Tier-B `Kernel`** (`script::kernel`) and only
  `Kernel::eval` runs per frame — the two-tier perf rule (`CLAUDE.md`): the
  scripting VM/compile never touches the hot path. Cyclic/forward refs read
  last frame's value (the usual dataflow rule); params publish first, then
  computed signals in order, then bindings read the bus.

- **Sources** are *reads* of scene state, referencing bodies by `StableId`:
  speed, spin, height, distance between two bodies, net contact force,
  contact count — or `Named`, a value someone else published on the bus
  (a script today; another node tomorrow).
- **Sinks** are *derived writes only*: a body's fill tint, its tracer-trail
  tint (both via the derived `SignalColorOverride` component, which the
  render sync prefers over authored `Appearance` and drops when the
  binding goes), or `Plot` (publish-only). **Authored state is never
  touched** — remove the binding and the authored look returns.
- **The bus** (`SignalBus`) is the wire protocol: every binding publishes
  its value under its name (with a rolling history the plot panel draws;
  recording pauses with the simulation). Scripts publish with
  `(signal-set name value)` and read with `(signal-get name)` — so a
  script can compute anything (e.g. `(touch-count i)`) and a `Named`
  binding turns it into color.
- **Gradients** come from the [`colorgrad`] crate (viridis, turbo, plasma,
  inferno, cool-warm), quantized to bands so a continuously varying signal
  re-tints only on band crossings instead of rebuilding materials every
  frame. User-authored gradients plug in later through the same
  `at(t) → color` contract.

[`colorgrad`]: https://crates.io/crates/colorgrad

## Governance & classification

| State | Class | Rules |
|---|---|---|
| `SignalBindings`, `SignalParams`, `ComputedSignals` | **config seam** (invariant-#4, like `GridSettings`) | UI edits directly; persisted in the scene's `EnvironmentRecord` (serde-defaulted — old files load); not undoable; bodies by `StableId` only |
| `SignalBus`, `ScriptSignals` | derived | rebuilt continuously; never persisted; bus hygiene drops entries whose binding/param/computed/script producer is gone |
| `CompiledSignals` | derived | the compiled kernels behind `ComputedSignals`, rebuilt by `recompile_signals` on change — keeps the *compile* step off the frame loop |
| `SignalColorOverride` | derived component | written change-detected by `evaluate_signals`; consumed by `material_sync`/`tracer`; removed with its binding |
| `BehaviorNode` / `NodeAttachment` / `NodeKind` | **authored** (like a body) | placeable dataflow nodes — the tracer tool today, sensors/actuators next; `StableId` + pose + optional body attachment, saved in `SceneRecord.nodes`, undoable via `SpawnNode`/`Delete` |

## Behavior nodes: dataflow as placeable, selectable entities

The dataflow's endpoints don't have to live in a panel — they can be
**placed in the scene**. A [`domain::node`](../src/domain/node.rs) is an
authored entity (its own `StableId`, pose, and optional attachment to a
body) that represents a piece of the graph:

- **Node tools** (toolbar): `Tracer` (key `Y`) draws a trail; `Sensor`
  (`N`) reads a scene quantity at its body and **publishes** a bus signal
  (a dataflow *input* port); `Actuator` (`U`) **reads** a bus signal and
  tints its body through a domain + gradient (an *output* port). A
  sensor+actuator pair sharing a signal name is the two placeable halves of
  a binding, wired by the bus — decompose a binding into nodes, or build a
  chain across bodies. Signal names are edited in the **node inspector**
  (the Signals dock's top block, undoable via `PropertyValue::NodeKind`).
  Each tool click attaches to the body under the cursor (rides it) or
  drops a free node.
- Nodes are **individually selectable** (`node_edit::pick_node`, after
  joint picking) and deletable/undoable like bodies — their glyph shows the
  selection state and a tether to the attached body. Right-clicking a node
  opens its **own context menu** (kind editor + Delete), distinct from the
  physical-object menu. A node is **deleted with its parent body** (cascade,
  like a joint), and a tracer attached to nothing is **inert**. The tracer's
  size, fade, and pattern (line / dots) are configurable.
- **Behavior copies with the base object.** Duplicating a body clones the
  tracer nodes attached to it (attachment remapped to the copy) *and* the
  signal bindings that reference it (`signal::remap_binding`, fresh names),
  so a duplicated object brings its whole behavior — undoably.

This is the first "a dataflow endpoint is its own tool" instance; every
future **sensor** (a reader glyph) and **actuator** (a writer glyph) is a
sibling `NodeKind` + tool, spawned through the same `ToolCommit::SpawnNode`
seam. The node canvas of the eventual editor wires these nodes' ports;
today they publish/read through the same named `SignalBus`.

The perf rule holds: the per-frame evaluator (`signal::evaluate_signals`)
is plain queries + arithmetic over the `physics::queries` facade — the
scripting VM never enters the frame loop. Scripts participate on their own
cold runs by publishing named values.

## Trajectory to the node editor

Deliberate seams for what comes next, so the editor is a UI change, not a
rearchitecture:

- **Enums become node kinds.** `SignalSource`/`SignalSink` variants are
  today's node palette; the map + gradient are the first two modulator
  nodes. A graph is `Vec<SignalBinding>` generalized to nodes + edges —
  the bus already names every wire.
- **Drag-a-property becomes a source.** The inspector/probe panels read
  the same facade quantities the sources do; "drag speed out of the probe
  panel" will mint a `SignalSource` the same way the Signals window's
  add-buttons do today.
- **Modulators lower to the Tier-B kernel.** When bindings grow
  expressions (P2), they compile through `script::kernel` — the
  allocation-free tape — not the VM.
- **More sinks accrete**: gizmo tints, per-vertex color mixing (SDF color
  blend), emissive, sim parameters (an *actuator* is a config write —
  same seam `sim-set` uses).

## Using it today

1. Select a body, open **Signals** (transport strip), click **add: speed**
   — the body tints by its speed through viridis. Adjust the domain
   drags; switch the gradient or the sink (fill / tracer / plot).
2. Distances need two selected bodies; **named** reads a script signal.
3. The plot panel (`\`) draws every binding's history under its bus name.
4. From a script: `(signal-set "excitement" (touch-count 0))` then bind
   *named* `excitement` to the body's fill.
