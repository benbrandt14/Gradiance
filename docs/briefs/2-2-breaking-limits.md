# Joints that break
Phase 2 · M20 · "Cams, planar contact, magnetism, **breaking limits**,
backlash (12)" — breaking limits · status: done

## Goal

A joint can be given a **break force**: while the simulation runs, if the
joint's reaction force exceeds it, the joint breaks — it stops constraining
its bodies and draws as broken. Undoing the run restores it. The joint's
reaction force is a readable quantity (facade query + signal source), so it
can be plotted and scripted like any other.

## Read first

- `crates/gradiance-domain/src/joint.rs` — `JointCommon` (≈L124, only
  `collide_connected` today), `JointDef`.
- `crates/gradiance-physics/src/joint_sync.rs` — how `JointDef` becomes the
  live avian joint entity (derived state).
- avian 0.7 `src/dynamics/joints/mod.rs` ≈L135–210 (in
  `~/.cargo/registry/src/*/avian2d-0.7.0/`) — `JointForces` and
  `JointDisabled`, with avian's own "break a joint" example.
- `crates/gradiance-command/src/lib.rs` ≈L170–190 — `push_boundary`: a run is
  one undo step that snapshots *authored* state at the play/pause boundary.
- `docs/briefs/2-1-joint-sensors.md` § Outcome — landed (#219). Its joint
  read path is the pattern: `PhysicsQueries::joint_state(entity, &IdIndex)`,
  `SignalSource::JointLength/JointAngle`, and the row table
  `ui/ports.rs::joint_sensors` the joint inspector renders.

## Facts (orchestrator, 2026-10-25, main `e631357`; updated after #219)

- avian provides `JointForces` (opt-in component on the joint entity:
  reaction force/torque after the solve) and `JointDisabled` (a marker that
  turns the constraint off and is reversible).
- The run boundary snapshots authored components. So **"broken" must be
  authored state** — written by a physics system during the run, exactly as
  the solver writes poses — or saving after a break and reloading would
  silently reconnect the joint, and undoing the run would have nothing to
  restore. The live `JointDisabled` is then *derived* from it by the sync
  (invariant 5: derived state is rebuilt from authored state, never saved).
  *Unverified:* that a physics-side write to a domain component during play
  is captured by `push_boundary` the same way poses are — confirm with a
  test before building on it.
- Authored joint fields are serde-persisted in `JointDef`; new fields need
  `#[serde(default)]` (scene format stays v6; check the goldens).

## Approach

1. Authored: `JointCommon.break_force: Option<Force>` (typed unit; `None` =
   unbreakable, the default) and a broken flag (field or marker component —
   your call, say why). Reflect-derived, serde-defaulted.
2. Physics: add `JointForces` to live joint entities that have a break
   force; one system in the fixed step compares magnitude to the threshold
   and sets the authored broken state. Sync maps broken → `JointDisabled`.
3. Read facade: `PhysicsQueries::joint_force(entity)` (typed `Force`) next
   to `joint_state`; `SignalSource::JointForce(StableId)` with arms in the
   same places 2-1 touched (bus-name tag, dimension, `read_source`, UI arms)
   and **one row** in `ports::joint_sensors` — the inspector picks it up
   with no other UI code (2-1 Outcome).
4. Editing: the break force is a field in the joint inspector (edit seam,
   `widgets::precise_drag`, `PropertyEditIntent`) — thinnest UI. A broken
   joint draws dashed/greyed in `render/joint_viz.rs`. "Repair" is out of
   scope (undo is the repair).
5. Script: the field rides the existing joint `PropertyValue::Joint` edit; if
   a joint verb takes keyword properties already, extend it; otherwise note
   it.

Invariants: 1–2 (editing the threshold is a command; breaking is simulation,
like motion, recorded by the run boundary); 5 (`JointDisabled` derived).

## Out of scope

Backlash, cams, planar contact (separate briefs), torque-based breaking
(record whether `JointForces` makes it trivial), repair UI, particle effects,
`docs/state.md`.

## Done when

- Integration test: a hinge with a low break force under a heavy load
  breaks during a run (bodies separate); `push_boundary` then undo restores
  the joint intact.
- Save→load round-trip of a broken joint stays broken.
- A joint with `break_force: None` never breaks under the same load.
- Goldens unchanged except explicit new fields. Gates green (workspace tests
  via CI if disk is tight).
- Roadmap M20 breaking-limits marked landed. Outcome filled.

## Outcome

**Status: done**, PR #220.

**The unverified fact holds.** `joint_breaking::a_play_time_write_to_a_joint_def_is_captured_by_the_run_boundary`
writes `JointDef` directly mid-run (as a physics system does), pauses, and
undoes; the write is in the run's snapshot and undo reverts it. The reason
is that `SceneRecord::capture` reads the live `JointDef`, and
`restore_diff` replaces any joint whose record differs. The comment there
said joints have "no simulation-evolved authored state". It now names
`broken` as the one exception.

What landed:
- **Domain**: `JointCommon.break_force: Option<Force>` and `broken: bool`,
  both `#[serde(default, skip_serializing_if = …)]`. A scene with no
  breakable joint serializes byte-identically, so the replay goldens are
  unchanged and there is no format bump. Also new: pure
  `exceeds_break_force` (unit-tested) and `DEFAULT_BREAK_FORCE` (100 N,
  the value used when "breakable" is first ticked).
- **Physics** (`joint_sync`): every derived joint gets `JointForces`.
  `JointDisabled` is derived from `broken` and inserted *before* the joint
  component, so avian's on-add observer never puts a broken joint into its
  graph. `break_overloaded_joints` runs in `FixedPostUpdate` in
  `PhysicsSystems::Last`, only while `Playing`, and sets the authored flag.
- **Read facade**: `PhysicsQueries::joint_force(entity) -> Option<Force>`.
  `SignalSource::JointForce` (`jforce@`, `Dimension::Force`) has arms in
  `read_source`, remap, and the three UI matches. `ports::joint_sensors`
  gained one row, `force` (N).
- **UI**: the joint inspector has a "breakable" checkbox and a break-force
  `precise_drag_unit`; both edit through `PropertyEditIntent`. A broken
  joint shows the note "broken (undo the run to repair)".
- **Render**: a broken joint skips its kind glyph, limits and motor. It
  draws a grey dashed ring at each half's anchor, and the two rings drift
  apart as the bodies separate.
- **Tests** (`tests/it/joint_breaking.rs`): the boundary fact; an
  overloaded world-pin hinge (5 N limit, ~10 N load) breaks, the body
  falls, and undo restores the joint intact and the body on its pin;
  `None` holds under the same load; `JointForce` reads about m·g;
  save→RON→load keeps the joint broken, `JointDisabled`, and free-falling.

Judgment calls:
- **Broken is a field, not a marker component.** `JointRecord` captures
  `JointDef` wholesale, so a field is persisted, undo-captured and reflected
  with no record or format change. A marker would have needed a new
  `JointRecord` field plus capture and spawn arms. The cost: a break writes
  `JointDef`, so `Changed<JointDef>` re-derives that joint once. That is
  harmless because it is disabled anyway.
- **Force measure is linear only**: `|JointForces::force()|` in N.
  Torque-based breaking would be trivial, because avian fills
  `JointForces::torque()` from the same writeback (a 2D scalar). It is out
  of scope. It would be a `break_torque` field plus one more comparison.
- **`JointForces` goes on every joint**, not only breakable ones (the brief
  said only breakable). The force row appears in every joint inspector, and
  a sensor that reads nothing on unbreakable joints would mislead. The
  writeback costs one multiply per joint.
- **A disabled joint reads 0 N.** avian keeps writing the last solve's
  stale multipliers for a disabled joint, so the facade applies the zero.
- Once broken, the connected bodies should collide again, because avian
  removes a disabled joint's graph edge, which is what filtered their
  contacts. This is untested; I read it from avian's source.

Not done / notes:
- **Script**: no joint verb takes keyword properties (`hinge`/`slider`/
  `spring` are positional) and no joint property-edit verb exists, so
  setting a break force from a script is not yet possible. The field rides
  `PropertyValue::Joint` like every other joint field. A
  `joint-set`-style verb is a follow-up.
- Editing the break force of an already broken joint does not repair it.
  Undo is the repair, and repair UI is out of scope.
- Workspace tests were left to CI: disk was 9 GB free, under the session's
  20 GB bar. Run locally: clippy, doc, fmt; unit tests for domain,
  physics, signal, ui, scene, render; the full `it` suite (254) and
  `boundaries`. Container rustc 1.97.0 = `RUST_PIN`.
