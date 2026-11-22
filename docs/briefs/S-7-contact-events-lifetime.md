# Contact events, removal on contact and time to live
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("on collide", "time to live", removal on contact) · status: done

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

Done (2026-11-22).

- **Domain:** authored `Lifetime { remove_on_contact, ttl }`
  (`domain::lifetime`), optional, saved in `BodyRecord.lifetime`, in undo
  records and the snapshot. `sanitized()` drops a time that is not positive
  and finite, and stores an empty lifetime as none. The message
  `ClearRemovals` lives in the domain too, because the command layer sends
  it and the physics layer answers it (no upward edge).
- **Physics** (`physics::lifetime`): every `Body` requires
  `CollisionEventsEnabled` (avian sends `CollisionStart` only on opt-in).
  After `PhysicsSystems::Last`, while the run plays, one system fills
  `ContactHits` and another marks `Removed` (+ `ColliderDisabled`,
  `RigidBodyDisabled`, `Visibility::Hidden`). The pair flag
  `STARTED_TOUCHING` is cleared inside the narrow phase, so the messages
  are the only edge source. A fluid region is never a removal target. The
  time to live counts `fixed timestep x relative speed` per step in a
  derived `RunAge`, with half a step of slack.
- **Removal is derived:** nothing despawns. `CommandStack::undo` sends
  `ClearRemovals` when it undoes a SIMULATE step; that clears the markers
  and every `RunAge`, and a ground stays hidden if the scenery hides
  grounds. A loaded scene respawns every body, so it needs no message.
  "Stop" in the brief has no separate state here: undo of the run is the
  rewind.
- **Signal:** `SignalSource::HitPulse(id)` (bus tag `hit`) and the
  quantity `hit` (1 on the step a contact starts, else 0) through
  `physics::queries` (`LiveBody::hit`).
- **Doorways:** `body.set_lifetime` (`(set-lifetime i contact ttl)`) and
  the inspector rows "remove what it touches" and "remove after" (seconds,
  `precise_drag`).
- **Tests** (`tests/it/lifetime.rs`): a ball on a remove-on-contact floor
  is hit, removed, hidden and still in place, and undo of the run brings it
  back at its start; a 1 s ttl body is removed after 59..61 frames; the
  `hit` pulse fires on landing and not at rest. The persistence round trip
  covers the new field.
