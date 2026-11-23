# Gamepad input on the signal bus
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("joystick") · status: done

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

A gamepad axis or button can drive anything a key drives: `(defpad name
axis)` publishes −1..1 on the bus, so a stick steers a motor or a thruster.

## Read first

- Brief N-2 Outcome and `crates/gradiance-domain/src/keys.rs` — key inputs.
- `crates/gradiance-interaction/src/graph_keys.rs` — where keys publish.

## Facts

- Bevy reads gamepads through its `bevy_gilrs` feature. *Unverified* that
  the workspace's Bevy features include it; adding a feature changes the lockfile.

## Approach

- `PadInput { name, source: Axis | Button, dead_zone }`, scene content
  like key inputs; `signal.pad` operation; a node-graph input block.

## Out of scope

Force feedback. Remapping UI beyond the block's fields.

## Done when

- Test with an injected gamepad event: the named signal reads the axis value.
- Gates green.

## Outcome

Done 2026-11-23 (PR on the S-19 branch).

- Fact checked: the workspace's Bevy default features include `gamepad`
  and `bevy_gilrs` (both already in `Cargo.lock`), so no lockfile change.
- `domain::pad`: the name table `PADS` (6 axes, 17 buttons),
  `pad_input_name`, `parse_pad_input`, `apply_dead_zone` (0 inside, the
  rest rescaled to reach 1, sign kept). Unit tests.
- The input is a node-graph block, `BlockOp::Pad { input, dead_zone }`,
  not a separate `PadInput` resource: like key inputs it is scene content
  in `ComputedSignals` (saved, one `settings` undo step). The graph IR has
  `NodeKind::Pad` and the catalog row `signal::pad`.
- Operation `signal.pad` (Lisp `defpad`, Seam::Config): an unknown input
  name or a dead zone outside 0..0.9 is an error.
- `interaction::graph_pads`: the first `Gamepad` entity's stick or button
  goes on the bus as `pad@<name>[/<dead zone>]` each frame, 0 unless the
  sim plays. Decisions `pad-dead-zone-in-bus-name`, `pad-first-gamepad`,
  `pad-default-dead-zone`.
- Node graph: Sources > Pad adds the block; its footer has the input
  picker and the dead zone. Its labels come from the string table through
  the new `text::tr(ctx, id)` (the table is shared with egui memory), so
  the `node_graph.rs` ratchet count does not rise.
- Tests: `tests/it/gamepad.rs` connects a pad and injects
  `RawGamepadEvent` axis and button events. The named signal reads -0.6,
  the default dead zone reads -0.5/0.9, a button reads 1, paused reads 0,
  and pad values record no undo step.
- Not checked: a physical gamepad (the container has none).
