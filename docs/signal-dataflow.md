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
source (read) ──▶ map [in_min, in_max] → t ──▶ [curve] → t' ──▶ gradient(t') ──▶ sink (derived write)
                          └────────────────── value ────────────────────────▶ SignalBus (name → value + history)
```

The optional **curve** (`SignalBinding.curve`, serde-defaulted so old scenes
load) is the Lightroom-style response transfer — control points reshaping the
normalized `t ∈ [0, 1]` before the gradient (`SignalBinding::transfer` is the
one place map + curve compose). The default identity line is the straight map,
so `None` and an untouched curve are indistinguishable in behaviour. It is
authored with `ui::curve::curve_editor`, embedded both in the binding row and
in the `BlockOp::Curve` modulation block's footer.

On the hot path a curve is **not** its control points: `signal::compile`
samples it once into a `gradiance_kernel::Lut`, and the tape's `Instr::Curve`
is a clamp plus a lerp into an out-of-line table. That is the two-tier rule
applied to a shape rather than an expression — the per-lookup segment search
the authoring form needs never enters the frame loop, and every curve costs the
same regardless of how many points were placed.

- **Params** (`defparam name value min max`) are tunable knobs — an
  auto-slider in the signal list beside the node canvas. Each publishes its value on the bus
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
- **Actuator sinks** (brief N-1) drive physics the same way: a joint's
  **motor speed** and **motor cap**, a body's **field strength** and
  **gravity scale**, a thruster node's **thrust** (brief N-3, the
  `Thrust` sink keyed by the node). `signal::drive_actuators` (one system, allocation-free
  in the steady state) writes the derived `physics::drive::SignalDrive` on
  the target; physics prefers it over the authored value
  (`retune_joints` for motors, `Fields` for strength, a derived
  `ConstantLinearAcceleration` for gravity scale). An actuator takes the
  **raw value** in its SI unit — no map, curve or gradient; shape it
  upstream with a computed block. Unbinding removes the drive and the
  authored value returns with no command. A drive never enables a disabled
  motor or adds a motor or field; a driven cap of 0 or less switches the
  motor off.
- **Key inputs** (brief N-2) are a source block: `BlockOp::Key { key,
  negative }` reads `1` while `key` is held **in play** (`-1` for
  `negative`, `0` when both are), lowered to the kernel over the bus inputs
  `key@<name>`. `interaction::graph_keys` publishes those each frame between
  `publish_sensor_refs` and `evaluate_computed` (derived: never saved,
  never undone). Paused, or while a field has the keyboard, they read `0`
  and the editor keeps every key; in play a graph-bound key is the graph's,
  and the editor's bare-key uses of it (a tool letter, `Space`, the arrow
  pan, a gesture's `Enter`/`Escape`) yield; a chord with a modifier does
  not. The block's footer captures its keys (click, press) and warns of
  each clash; `(defkey name key [negative])` declares one from a script.
  The key binding is the block, so it is scene content like any block.
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
| `SignalBindings`, `SignalParams`, `ComputedSignals` | **config seam** (invariant-#4, like `GridSettings`) | UI edits directly; persisted in the scene's `EnvironmentRecord` (serde-defaulted — old files load); scene content, so a settled edit is one `settings` undo step (U-2c); bodies by `StableId` only |
| `SignalBus`, `ScriptSignals` | derived | rebuilt continuously; never persisted; bus hygiene drops entries whose binding/param/computed/script producer is gone |
| `CompiledSignals` | derived | the compiled kernels behind `ComputedSignals`, rebuilt by `recompile_signals` on change — keeps the *compile* step off the frame loop |
| `SignalColorOverride` | derived component | written change-detected by `evaluate_signals`; consumed by `material_sync`/`tracer`; removed with its binding |
| `key@<name>` bus inputs, `GraphKeys` | derived | the held state of each key a key block reads, published by `interaction::graph_keys::publish_graph_keys` (allocation-free; the bound set is rebuilt only when `ComputedSignals` changes); never saved or undone |
| `SignalDrive` | derived component (`physics::drive`) | written by `drive_actuators` on a joint, body or thruster node; consumed by `retune_joints`, `Fields`, `drive_gravity_scale`, `apply_thrusters`; removed with its last actuator binding; never saved or undone |
| `LaserBeam` | derived component (`physics::laser`) | a laser node's traced path, hit, distance and bus names; written by `trace_lasers` after each step, published by `publish_lasers`; never saved or undone |
| `BehaviorNode` / `NodeAttachment` / `NodeKind` | **authored** (like a body) | placeable dataflow nodes — the tracer, the thruster and the laser; `StableId` + pose + optional body attachment, saved in `SceneRecord.nodes`, undoable via `SpawnNode`/`Delete` |

## Sensors & actuators are object ports, not tools

Sensors and actuators are **properties of objects**, not placeable things. A
body exposes named **ports**: its **sensor ports** (read-only) are the scene
reads it publishes — speed, spin, height, pos-x, contact force, contact count
([`SignalSource`]) — and its **actuator ports** (writable) are the color
channels a bound signal can drive — fill, tracer trail ([`SignalSink`]). The
single catalog lives in [`ui::ports`](../crates/gradiance-ui/src/ports.rs)
(`body_sensors`/`body_actuators`), reused by the inspector's live readouts and
the node canvas's pins, and read through the one shared reader
[`signal::read_source`](../crates/gradiance-signal/src/lib.rs) — there is no parallel "read a
body quantity" implementation, and no `SensorQuantity`/`ActuatorTarget` enum.

A **wire between ports is a [`SignalBinding`]**: a source (a sensor port, a
param, or a computed signal) → a map + gradient → a sink (an actuator port or
the plot). Bindings are the single config-seam currency — edited directly,
persisted with the scene, one settled undo step per edit — and duplicating a body copies the
bindings that reference it (`signal::remap_binding`, fresh names), so behavior
copies with the base object.

## The placeable tracer node

The one remaining *placeable* dataflow entity is the **tracer** — a trajectory
probe. A [`domain::node`](../crates/gradiance-domain/src/node.rs) is an authored entity
(`StableId`, pose, optional body attachment) whose only [`NodeKind`] is
`Tracer`. The **Tracer tool** (key `Y`) drops one on the body under the cursor
(it rides the body) or free; nodes are individually selectable
(`node_edit::pick_node`, after joint picking), have their own right-click menu
(size / fade / pattern + Delete), cascade-delete with their parent body, and a
tracer attached to nothing is inert. A tracer's *trail color* is an actuator
port on the body it rides (the `TracerColor` sink), so wiring drives it like
any other actuator.

## The placeable thruster node (brief N-3)

A **thruster** is the second placeable node kind (`NodeKind::Thruster`,
[`domain::thruster`](../crates/gradiance-domain/src/thruster.rs)): attached to
a body, it pushes the body with its authored **thrust** (N) along its
**axis** (an angle in the body's frame, so it turns with the body), at the
node's own point. Off the centre of mass the push also turns the body. The
**Thruster tool** (no key: the editor keymap is not settled) places one: press
on a body where it pushes; a click pushes up, a drag aims the push along the
drag. Script: `(thruster body x y angle thrust)` (`node.thruster`), the point
and angle in world terms. Thrust and axis are edited in the inspector, the
node's context menu and the Signals list: one undo step per gesture.

The push is **derived**: `physics::thrust::apply_thrusters`, one system in
`FixedUpdate` (once per physics step, gated like the field forces), reads
each thruster node, its body by id, and a `SignalDrive` thrust if a binding
sets one. It is `O(T)` over the behavior nodes with one id lookup each and
allocates nothing. A free thruster, one whose body is gone and one on a body
that is not dynamic push nothing. The canvas shows a **thruster block** (one
input pin, thrust in N) while a wire names it or its body is on the canvas;
the glyph draws a flame out of the thruster's back, scaled by the thrust it
pushes with. Duplicating a body copies its thruster and the bindings that
drive it.

## The placeable laser node (brief N-8)

A **laser** is the third placeable node kind (`NodeKind::Laser`,
[`domain::laser`](../crates/gradiance-domain/src/laser.rs)): it casts a ray
from its point along its axis (in the body's frame when it rides a body, a
world angle when free), reflects off every solid body it hits (angle in =
angle out; the pure reflection is `geometry::beam`), and ends on its
**target** body, after its last **bounce** (default 8, cap 32) or at its
**range** (default 50 m). Sensors (fluid regions) and the laser's own body
do not stop it; a laser on a body hits only bodies that share its collision
layer. It is a **sensor that publishes on the bus** under its authored
**name**: `<name>` reads `1` while the beam ends on the target and `0`
otherwise; `<name>.distance` is the beam's path length to where it ends
(m). A binding reads them as a `Named` source and a computed block as
inputs, so a light gate or a reflection puzzle drives any actuator sink.

The **Laser tool** (no key) places one: press where it sits, drag to aim,
release; releasing over another body makes it the target. Script:
`(laser body x y angle target name)` (`node.laser`; `-1` for a free laser
or no target). Name, axis, range and bounces are edited in the inspector,
the node menu and the Signals list (one undo step per gesture); the target
can be cleared there.

The beam is **derived**: `physics::laser::trace_lasers`, one system in
`FixedPostUpdate` after avian's step (every fixed tick, paused too), casts
one ray per segment through avian's `SpatialQuery`
(`physics::queries::cast_beam`) and writes the `LaserBeam` component (the
path, hit, distance and the two bus names); `signal::publish_lasers` puts
the values on the bus before the computed signals run; the overlay draws
the path and an accent ring on a hit. Cost: `O(L × (B + 1))` ray casts per
step for `L` lasers with at most `B` bounces; the path buffer is reused, so
nothing allocates in the steady state. While paused the casts see the
colliders as the last step left them.

## The node-graph canvas

The Simulink-style editor this substrate has grown toward now ships as a
screen-**bottom docked** canvas ([`ui::node_graph`](../crates/gradiance-ui/src/node_graph.rs),
toolbar **⬡ Graph**), built on the [`egui-snarl`] node-graph widget — the one major
node editor tracking our pinned egui 0.36 (`egui_node_graph2` is stuck on
0.29 and can't share the bevy_egui context). snarl owns the box layout,
pan/zoom, and the drag-to-connect gesture; `ui::node_graph` is the **adapter**
between it and the ECS dataflow. The canvas **captures its own input** (it does
not click/scroll/resize through to the scene — its rect feeds `PointerOverUi`),
so pan/zoom is independent of the scene camera; a header **⛶ fit** button
zoom-to-fits the whole graph. The look is vvvv-flavoured — sharp corners, flat
fills, thin wires, and a faint **dot-grid that pans and zooms with the canvas**.
**Objects are the nodes**:

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
palette — **Input** (Parameter, Constant, Time, Oscillator, Key), **Modulation**
(Gain, Sum, Sub, Product, Min, Max, Abs), **Output** (the Scope). Modulation blocks are `ComputedSignal`s
carrying a structured `BlockOp` that lowers to the existing `SignalExpr`/kernel;
wiring a named producer (param / computed / block) into an operand pin sets that
operand and re-lowers. **Right-click a block** ▸ Remove / Delete; a block's **footer** configures it
in place — a body edits the domain + gradient of each wire driving it, a
modulation block edits its constants (k / amp / freq), and a param block carries
its tuning slider (the same knob as the signal list) — Simulink-style
double-click-to-configure, all editing the same authored state.

A **wire is a [`SignalBinding`]**: dragging a body's sensor output onto another
body's actuator input creates one (`source → sink`), and dragging the wire off
deletes it. Blocks show their **type** — a body defaults to its **shape kind** (box /
circle / polygon …) so several bodies stay distinct, `⊙ param`, `ƒ signal` —
with an optional custom name (an editable field in a body block); a body is added
explicitly via its right-click **Add to node editor** — bodies never auto-appear
on selection, so the canvas stays uncluttered — and the selected body's block is
highlighted. Selection is bidirectional: a body block's **⌖ locate** button
selects that object in the scene (so the inspector follows), the reverse of the
scene-selection highlight. The snarl graph is **reconciled from the scene every frame**
(`node_graph::reconcile`): a block per **added** or binding-referenced body, plus
every param/computed, keyed by `GraphKey` so dragged positions persist; the wires
are rebuilt from the bindings — the ECS is the source of truth, not snarl's own
graph. Wires are right-angle (Simulink), so a body's own sensor→actuator
self-wire routes around it. Layout is pure editor view-state in the `NodeGraph`
resource (never persisted). Wiring edits `SignalBindings` directly (config-seam,
like the signal list) — no placeable entity, one currency. Usable without
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
  derived per-field shadow that respects invariant #5 — `SignalDrive` is the
  first, brief N-1), gizmo tints, emissive, sim parameters — each a new
  catalog entry in `ui::ports` + a `SignalSink`. Joints have a minimal block
  since N-1: sensor outputs, and motor speed / cap inputs on a motorised
  hinge or slider.

## Using it today

1. Select a body, open **Signals** (transport strip), click **add: speed**
   — the body tints by its speed through viridis. Adjust the domain
   drags; switch the gradient or the sink (fill / tracer / plot).
2. Distances need two selected bodies; **named** reads a script signal.
3. The plot panel (`\`) draws every plottable signal's history under its bus
   name; a **signals** picker toggles each series and the X axis defaults to
   time.
4. From a script: `(signal-set "excitement" (touch-count 0))` then bind
   *named* `excitement` to the body's fill.
5. Right-click two bodies ▸ **Add to node editor**, open **⬡ Graph** (docks at
   the bottom), and drag one body's **speed** output onto another's **fill**
   input — the wire is a binding and the target tints from the source's
   reading. No scripting needed.
