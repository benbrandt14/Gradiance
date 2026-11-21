# Laser block and node-canvas follow-ups
Phase Q (follow-ups) · N-8 Outcome "no laser block on the node canvas",
"the target can only be set by the tool or the operation", "two lasers with
one name share one bus value"; N-1 Outcome "the joint block has no 'add to
node editor' entry" · status: done

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

1. **A laser block on the node canvas.** A laser shows as a block with two
   output pins, `hit` (0/1) and `distance` (m), with live readouts. Wiring
   a pin into an actuator makes the same binding a person makes today by
   typing the laser's bus name (`SignalSource::Named`). The block shows
   when a wire reads the laser's names, when its body is on the canvas, or
   when the person adds it.
2. **Add to node editor** for a joint and for a node (laser, thruster),
   beside the body entry that exists.
3. **Set a laser's target in the editor:** a "target: selection" button
   in the laser editor sets the target to the selected body (one undo
   step, the same property path that "clear" uses).
4. **A new laser gets a free name:** `laser`, then `laser2`, `laser3`, …,
   so two lasers drawn one after the other publish two values. A name
   typed by the person or given to the operation is kept as typed.

## Read first

1. `docs/signal-dataflow.md`: the laser section and the bus names.
2. `crates/gradiance-ui/src/node_graph.rs:54-140` (`GraphKey`,
   `NodeData`), `:270-300` (`source_pin`, `sink_pin`), `:740-800` (the
   block list).
3. `crates/gradiance-ui/src/widgets.rs:490-560` (`laser_editor`).
4. `docs/ui-design.md`: block and field rules.

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- The laser publishes `<name>` and `<name>.distance`
  (`crates/gradiance-signal/src/lib.rs:266`, `publish_lasers`;
  `DISTANCE_SUFFIX` at `crates/gradiance-domain/src/laser.rs:44`).
  `DEFAULT_NAME = "laser"` (`laser.rs:41`); `Laser.target:
  Option<StableId>` (`laser.rs:52`).
- Decision `laser-signals` (archive) chose bus names over sensor ports and
  noted "no laser block on the node canvas yet". The block reads those
  names; it adds no `SignalSource` variant.
- `source_pin` maps every `SignalSource::Named(name)` to
  `GraphKey::Param(name)` (`node_graph.rs:272`). A laser block must claim
  a `Named` source whose name is a live laser's `<name>` or
  `<name>.distance` before that fallback.
- The thruster block is the pattern for a node block: `NodeData::Thruster
  { id }` (`node_graph.rs:97`), keyed `GraphKey::Node(id)` (`:131`),
  listed when a wire names it or its body is on the canvas (`:767-778`).
- "Add to node editor" exists for bodies only
  (`crates/gradiance-ui/src/context_menu.rs:508-518`, calling
  `node_graph.add_body`); the canvas keeps the added bodies
  (`node_graph.rs:219`).
- `laser_editor` says "here it can only be cleared"
  (`crates/gradiance-ui/src/widgets.rs:500-505`); the editor's change is
  returned as a new `NodeKind` and committed on the edit seam.

## Approach

- `NodeData::Laser { id, name }` under `GraphKey::Node(id)`, two outputs,
  no inputs. Its pins resolve to `SignalSource::Named(name)` and
  `Named(name + DISTANCE_SUFFIX)`; a dragged wire from a pin creates the
  binding through the existing Signals path (config seam, one `settings`
  step). Readouts come from the bus, as the param block's do.
- A rename of the laser leaves old wires reading the old name (the model
  is name-based; G-3 may change that). The block shows such a wire as
  dangling, as a param block does for a missing name.
- "Add to node editor" for the selected joint or node: a set of added
  joints and nodes in the canvas view state, as for bodies (view state,
  not saved: the same rule as the body set).
- The target button writes the laser `NodeKind` with `target: Some(id)`
  through the editor's existing commit; disabled when the selection is not
  one other body.
- Free names: where a laser record is built from the tool, pick the first
  of `laser`, `laser2`, … not used by a live laser. The operation keeps
  its `name` param as given.

## Tests

- UI unit tests in `node_graph.rs`: a laser named `gate` gives a block with
  two pins; a binding from `Named("gate")` resolves to the laser block's
  pin 0, from `Named("gate.distance")` to pin 1, from an unrelated name to
  the param block.
- `tests/it/lasers.rs`: two lasers drawn with the tool publish two bus
  values; the target button sets the target in one undo step and the hit
  reads 1 after the next step.
- A context-menu test for "Add to node editor" on a joint (the joint block
  shows without a wire).
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-ui --lib`, the
  touched `--test it` modules.
- One lavapipe screenshot of the laser block wired into a gravity-scale
  pin, on the PR (not committed).

## Out of scope

- A sensor-port model for lasers (`SignalSource` variants), node-canvas
  layout persistence, joint editing on the canvas (retained 2-1 items).
- A ray-cast script verb.
- `gradiance-graph` (held draft).

## Outcome

Done (2026-11-21), except the lavapipe screenshot.

- `NodeData::Laser { id, name }` (outputs `hit`, `distance` with unit m,
  live readouts off the bus). `source_pin` now takes the live lasers
  (`NodeGraph::lasers`, refreshed each frame in `prepare`): a `Named`
  source matching `<name>` or `<name>.distance` resolves to the block's pin
  0 or 1 before the param fallback. Operand wires resolve the same way.
  `collect_desired` → `node_blocks` shows a laser read by a wire or a
  modulation block, riding a body on the canvas, or added.
- **Add to node editor** for a joint (joint section) and for a thruster or
  laser (node menu). One `added` set holds bodies, joints and nodes
  (`NodeGraph::add`). A joint or node block that was added offers
  "Remove from node editor".
- **Target from the selection.** `node_kind_editor` takes `pick` (the one
  other selected non-node object). The laser row's "selection" button sets
  `target` as one edit. The inspector and the node menu pass a pick; the
  Signals list passes none.
- **Free names:** `laser::free_name` (`laser`, `laser2`, …), used by the
  Laser tool through `ToolWorld::free_laser_name`. The operation keeps its
  name as given.
- Tests: `a_laser_block_claims_its_bus_names` (node_graph), a kittest
  button test (widgets), `free_name` unit test, and
  `ui_input::two_drawn_lasers_take_two_names`.
- Not done: the context-menu "Add to node editor on a joint" test (no
  headless egui pass reaches the menu) and the PR screenshot.
