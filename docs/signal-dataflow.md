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
