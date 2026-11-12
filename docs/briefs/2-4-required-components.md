# The body archetype, enforced
Phase 2 · M16 residual · "Deferred (Bevy audit): enforce the authored
`Body`/`Joint` archetype with **required components** (`#[require(...)]`)
once the domain types carry sensible `Default`s" · status: done

## Goal

Spawning a `Body` or `Joint` marker can never produce an entity missing its
authored companions: the markers declare `#[require(...)]` for the
components every body/joint must have, so a hand-written spawn (a test, a
future script path, a future importer) gets a valid archetype. Behaviour of
the app is otherwise unchanged.

## Read first

- `docs/bevy19-notes.md` — required-components syntax and semantics in 0.19
  (your training data is probably stale).
- `crates/gradiance-domain/src/lib.rs` ≈L20–30 — the `Body` / `Joint`
  markers (`#[derive(Component, Default)]`, no requires today).
- `crates/gradiance-scene/src/records.rs` ≈L98–122 (`BodyRecord::spawn`) and
  ≈L300 (the joint spawn) — the one place authored entities are built today.
- `crates/gradiance-geometry/src/shape.rs` — `ShapeDef` (no `Default`).

## Facts (orchestrator, 2026-10-25)

- `BodyRecord::spawn` inserts `Body, StableId, Transform, ShapeDef,
  Appearance, DepthBand` and then the physics components. Spawning is
  already centralised, so the value here is **hardening**, not a bug fix.
- `ShapeDef` is a `Default`-less enum — the roadmap's stated blocker. A
  `Default` (e.g. a unit circle or small box) must be a deliberate, documented
  choice; it is only ever observed if someone spawns `Body` alone.
- `StableId` must **not** be defaulted to a fixed value (invariant 3:
  identity). If required, its default must mint a fresh UUID — check whether
  `StableId::default()` already does; if not, do not require it and say why.
- *Unverified:* whether any existing code spawns `Body` without one of these
  (tests especially); requiring components could mask a missing-field bug by
  filling a default. Grep test helpers.

## Approach

1. Give the domain types that lack one a sensible `Default` (documented).
2. `#[require(...)]` on `Body` and `Joint` for the authored companions;
   leave derived components (colliders, meshes) out — those are rebuilt by
   sync systems (invariant 5).
3. A test: spawning bare `Body` (+ a `StableId` if not required) yields an
   entity with every required component, and the derived sync builds a
   collider for it.

## Out of scope

Changing what `BodyRecord::spawn` inserts, the persistence format, any
derived-state sync, `docs/state.md`.

## Done when

- The bare-spawn test passes; all existing tests unchanged.
- Gates green (workspace via CI if disk is tight).
- Roadmap M16 "required components" line marked landed. Outcome filled.

## Outcome

**Status: done** (PR #222) — landed as briefed, with `Joint` deliberately
narrower than `Body`.

- `Body` requires: `StableId`, `Transform`, `ShapeDef`, `Appearance`,
  `DepthBand`, and the authored avian components `RigidBody` (Dynamic),
  `Friction`, `Restitution`, `ColliderDensity`, `GravityScale` at the
  `BodyPhysics::default()` values (via the `props::default_*` fns), so a
  bare body's live physics matches what `BodyPhysics::capture` records.
  Optional markers (`Sensor`, `LockedAxes`) are absent-by-default, so not
  required. No derived component is required (invariant 5).
- `Joint` requires only `StableId` + `Transform`. `JointDef` is **not**
  required: it must name a real `body_a`, and no default can — a defaulted
  joint would silently reference nothing. Left visibly incomplete instead.
- `StableId::default()` already mints a fresh UUID (`StableId::new`), so
  requiring it is identity-safe (invariant 3); the test asserts two bare
  spawns get distinct ids and both land in `IdIndex`.
- `ShapeDef` gained `Default` = `Box { 1.0, 1.0 }` (1 m unit square,
  documented in `geometry::shape`).
- Grep found no existing code spawning `Body`/`Joint` outside
  `BodyRecord::spawn` / `JointRecord::spawn`, so nothing is newly masked.
  `BodyRecord::spawn` is unchanged; for static bodies the required
  `RigidBody::Dynamic` is now inserted first and then replaced by
  `insert_into` in the same exclusive call — avian handles a `RigidBody`
  re-insert (its type-change observers), and the explicit-override test
  pins the result.
- Tests (`tests/it/physics_sync.rs`): bare `Body` spawn gets the full
  archetype + a derived collider + round-trips through capture; explicit
  values beat defaults; bare `Joint` gets identity but no `JointDef`.
- Roadmap M16 line marked landed.

