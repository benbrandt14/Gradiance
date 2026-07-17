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

## Sensors & actuators are object ports, not tools

Sensors and actuators are **properties of objects**, not placeable things. A
body exposes named **ports**: its **sensor ports** (read-only) are the scene
reads it publishes — speed, spin, height, pos-x, contact force, contact count
([`SignalSource`]) — and its **actuator ports** (writable) are the color
channels a bound signal can drive — fill, tracer trail ([`SignalSink`]). The
single catalog lives in [`ui::ports`](../src/ui/ports.rs)
(`body_sensors`/`body_actuators`), reused by the inspector's live readouts and
the node canvas's pins, and read through the one shared reader
[`signal::read_source`](../src/signal/mod.rs) — there is no parallel "read a
body quantity" implementation, and no `SensorQuantity`/`ActuatorTarget` enum.

A **wire between ports is a [`SignalBinding`]**: a source (a sensor port, a
param, or a computed signal) → a map + gradient → a sink (an actuator port or
the plot). Bindings are the single config-seam currency — edited directly,
persisted with the scene, not undoable — and duplicating a body copies the
bindings that reference it (`signal::remap_binding`, fresh names), so behavior
copies with the base object.

## The placeable tracer node

The one remaining *placeable* dataflow entity is the **tracer** — a trajectory
probe. A [`domain::node`](../src/domain/node.rs) is an authored entity
(`StableId`, pose, optional body attachment) whose only [`NodeKind`] is
`Tracer`. The **Tracer tool** (key `Y`) drops one on the body under the cursor
(it rides the body) or free; nodes are individually selectable
(`node_edit::pick_node`, after joint picking), have their own right-click menu
(size / fade / pattern + Delete), cascade-delete with their parent body, and a
tracer attached to nothing is inert. A tracer's *trail color* is an actuator
port on the body it rides (the `TracerColor` sink), so wiring drives it like
any other actuator.

## The node-graph canvas

The Simulink-style editor this substrate has grown toward now ships as a
screen-**bottom docked** canvas ([`ui::node_graph`](../src/ui/node_graph.rs),
toolbar **⬡ Graph**), built on the [`egui-snarl`] node-graph widget — the one major
node editor tracking our pinned egui 0.35 (`egui_node_graph2` is stuck on
0.29 and can't share the bevy_egui context). snarl owns the box layout,
pan/zoom, and the drag-to-connect gesture; `ui::node_graph` is the **adapter**
between it and the ECS dataflow. **Objects are the nodes**:

[`egui-snarl`]: https://crates.io/crates/egui-snarl

- a **body** is a block whose **outputs are its sensor ports** (speed, spin,
  height, pos-x, contact force, contact count) and **inputs are its actuator
  ports** (fill, tracer color) — Simulink blocks, Algodoo per-object behavior;
- a **param** (`defparam`) is a producer block (one output);
- a **computed signal** (`defsignal`) is a modulator block (inputs = the names
  its expression reads, one output);
- a singleton **Scope** block is the Live Plot as a sink — wiring a signal into
  it makes a `SignalSink::Plot` binding.

Every pin shows its **live value** next to the port name (the Simulink "watch
the signal flow" readout), computed through `read_source` + the bus. Authoring
is in-canvas too: **right-click the background** for a categorized **Add block**
palette — **Input** (Parameter, Time, Oscillator), **Modulation** (Gain, Sum,
Product), **Output** (the Scope). Modulation blocks are `ComputedSignal`s
carrying a structured `BlockOp` that lowers to the existing `SignalExpr`/kernel;
wiring a named producer (param / computed / block) into an operand pin sets that
operand and re-lowers. **Right-click a block** ▸ Remove / Delete; a body block's
**footer** edits the domain + gradient of each wire driving it and a modulation
block's footer edits its constants (k / amp / freq) — Simulink-style
double-click-to-configure, all editing the same authored state.

A **wire is a [`SignalBinding`]**: dragging a body's sensor output onto another
body's actuator input creates one (`source → sink`), and dragging the wire off
deletes it. Blocks show their **type** ("body" / `⊙ param` / `ƒ signal`) with an
optional custom name (an editable field in a body block); a body is added
explicitly via its right-click **Add to node editor** — bodies never auto-appear
on selection, so the canvas stays uncluttered — and the selected body's block is
highlighted. The snarl graph is **reconciled from the scene every frame**
(`node_graph::reconcile`): a block per **added** or binding-referenced body, plus
every param/computed, keyed by `GraphKey` so dragged positions persist; the wires
are rebuilt from the bindings — the ECS is the source of truth, not snarl's own
graph. Wires are right-angle (Simulink), so a body's own sensor→actuator
self-wire routes around it. Layout is pure editor view-state in the `NodeGraph`
resource (never persisted). Wiring edits `SignalBindings` directly (config-seam,
like the Signals dock) — no placeable entity, one currency. Usable without
scripting: the sensor → map + gradient → actuator loop is entirely UI-driven.

The perf rule holds: the per-frame evaluator (`signal::evaluate_signals`)
is plain queries + arithmetic over the `physics::queries` facade — the
scripting VM never enters the frame loop. Scripts participate on their own
cold runs by publishing named values.

## Trajectory to the node editor

The canvas landed as a UI change over the existing model (as designed —
below), not a rearchitecture. What still accretes on the same seams:

- **More ports.** The canvas renders bodies (sensor/actuator ports), params,
  and computed signals; new sensor `SignalSource`s / actuator `SignalSink`s
  appear as pins for free once added to the `ui::ports` catalog.
- **Drag-a-property becomes a wire.** The inspector's sensor readouts and the
  canvas's output pins share one port catalog (`ui::ports`); dragging a port
  out of the inspector to bind it is the same `SignalBinding` the canvas makes.
- **Modulators lower to the Tier-B kernel.** Computed signals already
  compile through `script::kernel` — the allocation-free tape — not the VM;
  richer expressions reuse that path.
- **More ports accrete**: writable authored scalars as actuator ports (a
  derived per-field shadow that respects invariant #5), gizmo tints, emissive,
  sim parameters — each a new catalog entry in `ui::ports` + a `SignalSink`.

## Using it today

1. Select a body, open **Signals** (transport strip), click **add: speed**
   — the body tints by its speed through viridis. Adjust the domain
   drags; switch the gradient or the sink (fill / tracer / plot).
2. Distances need two selected bodies; **named** reads a script signal.
3. The plot panel (`\`) draws every binding's history under its bus name.
4. From a script: `(signal-set "excitement" (touch-count 0))` then bind
   *named* `excitement` to the body's fill.
5. Right-click two bodies ▸ **Add to node editor**, open **⬡ Graph** (docks at
   the bottom), and drag one body's **speed** output onto another's **fill**
   input — the wire is a binding and the target tints from the source's
   reading. No scripting needed.
