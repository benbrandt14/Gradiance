# Gamepad input on the signal bus
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("joystick") · status: ready

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
