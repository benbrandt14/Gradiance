# Signal bind and unbind operations
Phase Q (follow-ups) · N-1 Outcome "no `signal.bind` / `signal.unbind`
operation" (also N-2, N-3) · status: done

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

A script, the CLI and MCP can make and remove a signal binding, as the
node graph and the Signals list do. `(bind "spin" "osc" "motor-speed" 0)`
binds the bus signal `osc` to joint 0's motor speed; `(unbind "spin")`
removes it. Each call is one `settings` undo step, like `defsignal`.
With these, the N-1/N-2/N-3/N-8 tests that push into `SignalBindings`
directly can use the verb, and a light gate or a key drive is scriptable
end to end.

The operations work on **today's signal model**: a binding is a
`SignalBinding` in the `SignalBindings` resource, named by its `name`.
The graph model (G-0..G-3; the G-0 decision and the G-1 IR crate are
merged as planning artifacts, #296/#297) would later move
binding storage into the graph IR; when it lands, these operations keep
their parameters and change only where they write. Do not read or depend
on `gradiance-graph`.

## Read first

1. `docs/scripting.md`: the verb recipe and the three verb rules
   (seam, no upward read, one snapshot per run).
2. `docs/signal-dataflow.md`: sources, sinks, actuator sinks, the bus.
3. `crates/gradiance-script/src/ops.rs`: `DefSignal`, `DefKey` and the
   `EditorRequest` path they use (the pattern to copy).
4. `crates/gradiance-domain/src/signal.rs:25-345`: `SignalSource`,
   `SignalSink`, `SignalBinding`, `SignalBindings`.

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- `SignalBinding { name, source, map, curve, gradient, sink }`
  (`crates/gradiance-domain/src/signal.rs:304`); `SignalBindings(Vec<_>)`
  (`:336`) is scene content: the `signals` row of `scene_content!`
  (`crates/gradiance-scene/src/records.rs:458-467`), so a direct write is
  recorded as one settled `settings` step by
  `command::commit_settings_edits`.
- Sources: `SignalSource` (`signal.rs:25`); `Named(String)` reads a bus
  signal; `SignalSource::of_quantity(name, Subjects)` (`signal.rs:428`)
  is the inverse of the quantity catalog for the body and joint reads
  (`speed`, `joint-angle`, `distance`, …).
- Sinks: `SignalSink` (`signal.rs:258`): `Fill`, `TracerColor`, `Plot`,
  `MotorSpeed`/`MotorCap` (a joint id), `FieldStrength`/`GravityScale`
  (a body id), `Thrust` (a node id). There is no name-to-sink function
  yet; the canvas pin labels ("motor speed", "field", "gravity",
  "thrust") are in `crates/gradiance-ui/src/ports.rs:59-83`, above the
  script layer.
- `SignalMap` default is `[0, 500]` (`signal.rs:81`); an actuator sink
  takes the raw value and ignores map, curve and gradient (N-1 decision
  `actuator-raw-value`).
- The pattern: `DefSignal` (`crates/gradiance-script/src/ops.rs:340`)
  lowers to `EditorRequest::Computed`, which `apply_editor_requests`
  (`ops.rs:541`) upserts by name into the resource. Operations register
  in `install` (`ops.rs:83`).
- Targets: `Target` (`crates/gradiance-op/src/target.rs:20`: index, id,
  `"selection"`, `{"at": …}`). The resolvers `body_of` and `joint_of` are
  `pub(crate)` in `crates/gradiance-command/src/ops/mod.rs:96` and `:138`;
  `SceneSnapshot` (`crates/gradiance-command/src/snapshot.rs:81`) holds
  `bodies`, `joints` and `nodes` in id order.
- Tests that bind directly today: `tests/it/actuators.rs:76` and `:90`;
  `tests/it/graph_keys.rs`.

## Approach

1. **Domain:** `SignalSink::of_name(name, id) -> Option<SignalSink>` and
   `SignalSink::name()`, one table for both directions (`fill`,
   `tracer`, `plot`, `motor-speed`, `motor-cap`, `field`, `gravity`,
   `thrust`); a unit test that every variant round-trips.
2. **Operations** in `gradiance-script/src/ops.rs`, config seam, undo
   `settings`, like `signal.computed`:
   - `signal.bind` / `(bind name source sink [target] [of] [min] [max])`.
     `source` is a bus signal name, or a catalog quantity name when `of`
     (one or two targets, the quantity's subjects) is given. `target`
     names the sink's object in the sink's own vocabulary: a joint index
     for the motor sinks, a node index for `thrust`, a body index
     otherwise; `plot` takes none. `min`/`max` set the map (defaults: the
     `SignalMap` default). Re-binding a name replaces it (upsert, as
     `defsignal`). A target that names nothing is an `OpError::Invalid`
     with the reason, never a silent no-op.
   - `signal.unbind` / `(unbind name)`: removes the binding of that name;
     an unknown name is an error.
   - One new `EditorRequest` arm each (`Bind(SignalBinding)`,
     `Unbind(String)`). No new mutation path: the resource write is the
     config seam that `defsignal` uses.
   - Make the resolvers that the script crate needs public (or add a
     `node_of` beside `joint_of`); script already depends on command, so
     no new dependency edge.
3. **Docs:** `docs/ops.md` regenerated; verb rows in `docs/scripting.md`;
   a line in `docs/signal-dataflow.md` ("bindings are also made by
   `signal.bind`").
4. Move the direct `SignalBindings` writes in `tests/it/actuators.rs`
   and `tests/it/graph_keys.rs` to the verb where that keeps the test's
   meaning.

## Tests

- `tests/it/signal_bind.rs` (new module in `tests/it/main.rs`): a script
  binds a `defsignal` oscillator to a hinge motor's speed with `bind`,
  the wheel follows the signal (as in `actuators.rs`), `unbind` stops
  the drive and the authored motor returns; each call is one `settings`
  step and undo removes / restores the binding; a quantity source
  (`(bind "v" "speed" "plot" -1 (list 0))` or the final signature)
  publishes the body's speed; a bad sink name, a missing target and an
  unknown unbind name are typed errors with no step.
- The registry conformance tests and the 6-1 undo matrix pick the two new
  operations up from their examples (no hand list).
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-domain`,
  `-p gradiance-script`, `--test it` for the touched modules.

## Out of scope

- `gradiance-graph` and the G-0..G-3 drafts: no reads, no dependency.
- Curves and gradients through the verb (the map is enough; colour
  shaping stays in the Signals list).
- New sinks or sources; the laser block (Q-4).

## Outcome

Done (2026-11-21, branch `claude/jolly-noether-su0a6h`).

- `SignalSink::NAMES` / `name()` / `of_name()` / `subject_of()` and the new
  `SinkSubject` (domain): one table for both directions, round-trip test.
- `signal.bind` `(bind name source sink [target] [of] [min] [max])` and
  `signal.unbind` `(unbind name)` in `gradiance-script/src/ops.rs`, config
  seam, undo `settings`; `EditorRequest::Bind` / `Unbind` upsert / remove
  in `SignalBindings`. A new `BindingNames` context mirror lets `unbind`
  refuse an unknown name. `body_of` / `joint_of` are now `pub` and a new
  `node_of` resolves a node target (command ops; no new edge).
- The example uses every param (the registry conformance test needs each
  declared field in the example, and `null` has no Scheme form), so it binds
  `speed` of body 0 to joint 0's motor. The conformance fixture gains one
  binding (`drive`) for `unbind`'s example.
- `tests/it/signal_bind.rs` (4 tests). `actuators.rs` and `graph_keys.rs`
  bind through the verb.
- Learned: the undo check must run paused. A playing motor scene records a
  `simulate` step on top of the settings step.
