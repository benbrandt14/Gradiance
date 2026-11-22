# Surface velocity (conveyor bodies)
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("material velocity") · status: done

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A body has an optional surface speed. Bodies that rest on it are carried
along its surface as on a belt, without the body itself moving.

## Read first

- `docs/bevy20-notes.md` — avian 0.8 notes.
- `crates/gradiance-physics/src/body_sync.rs` — where collider derived
  data is built.
- `crates/gradiance-domain/src/fluid.rs` — the optional authored component
  pattern.

## Facts

- *Unverified:* avian 0.8 contact modification hooks and a per-contact
  tangent speed. **First step:** confirm the API in the avian 0.8 source.
  If it is absent, stop and return the brief with the finding. Do not
  emulate it with forces.

## Approach

- Authored `SurfaceSpeed(Velocity)` (m/s along the outline, positive =
  counter-clockwise), inspector row, `(set-surface-speed i v)`.
- One collision hook sets the contact tangent speed.

## Out of scope

Per-edge speeds. Animated belt visuals.

## Done when

- Behaviour test: a box on a 1 m/s static conveyor reaches 1 m/s ± 5 %.
- fmt, clippy, tests, doc gate green.

## Outcome

Done (2026-11-22).

- **Verified first:** avian 0.8 has the API. `CollisionHooks::modify_contacts`
  gets each `ContactPair`, and every `ContactManifold` has a 2D
  `tangent_speed` that the friction solver adds to the relative tangent
  speed (`dynamics/solver/contact/tangent_part.rs`). No force emulation.
- **Domain:** authored `SurfaceSpeed(Velocity)` (`domain::surface`),
  optional, saved in `BodyRecord.surface`, undoable, held to ±100 m/s;
  0 stores no component.
- **Sign:** positive runs counter-clockwise along the outline, as the
  brief says. Thus on the top of a body a positive speed carries a load to
  the left. For both orders of the pair, a body's speed adds `+S` to the
  manifold's `tangent_speed` (derivation in `physics::surface`); two
  conveyors in contact add.
- **Physics** (`physics::surface`): `GradianceHooks` is the app's one
  collision-hook set (`PhysicsPlugins::with_collision_hooks`).
  **Upstream defect:** avian 0.8 reads the modify flag only when a
  collider proxy is made, so every `Body` requires
  `ActiveCollisionHooks::MODIFY_CONTACTS` (`docs/upstream-watch.md`).
- **Doorways:** `body.set_surface_speed` (`(set-surface-speed i v)`) and
  the inspector row "surface speed" (`precise_drag`, one undo step).
- **Tests** (`tests/it/conveyor.rs`): a box on a static belt at -1 m/s
  moves right at 1 m/s ± 5 % and the belt stays put; at +2 m/s it moves
  left at 2 m/s; the verb is one undo step and 0 clears it. The
  persistence round trip covers the field.

