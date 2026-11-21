# Contact events, removal on contact and time to live
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("on collide", "time to live", removal on contact) · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A body can publish a hit pulse on the signal bus when a contact starts. A
body can be marked to remove whatever it touches, or to remove itself after
a time. Removal lasts for the run; undo of the run brings everything back.

## Read first

- `crates/gradiance-domain/src/signal.rs:25` — `ContactCount` and
  `ContactForce` sources.
- `crates/gradiance-command/src/lib.rs:190` — run recording.
- CLAUDE.md invariant 1 and the two-tier rule.
- `crates/gradiance-physics/src/laser.rs` — a per-step derived system.

## Facts

- The reference app runs a script on collide. Here the VM never runs per
  frame, so the hook is a signal edge (Tier B), not a callback.
- Despawning an authored entity during a run is a world mutation outside
  commands. A derived `Removed` marker that disables the collider and hides
  the mesh is not: it is derived state, cleared when the run is undone.

## Approach

- `SignalSource::HitPulse(StableId)`: 1 on the step a contact starts, else 0.
- Authored `Lifetime { remove_on_contact: bool, ttl: Option<Time> }`,
  optional, inspector + `(set-lifetime i …)`.
- One `FixedPostUpdate` system adds the derived marker; `body_sync` honours
  it. Undo of a run, stop, or load clears all markers.

## Out of scope

Script callbacks. Spawning during a run (emitters).

## Done when

- Behaviour test: a ball dropped on a remove-on-contact floor is hidden and
  non-colliding after impact; undo restores it.
- Test: a 1 s ttl body is gone at 1.0 s ± one step.
- fmt, clippy, tests, doc gate green.

## Outcome
