# Joints that break
Phase 2 · M20 · "Cams, planar contact, magnetism, **breaking limits**,
backlash (12)" — breaking limits · status: ready

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
- `docs/briefs/2-1-joint-sensors.md` — if it has landed, reuse its joint
  read path; if not, this brief adds the force read itself.

## Facts (orchestrator, 2026-10-25, main `e631357`)

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
3. Read facade: `PhysicsQueries::joint_force(entity)` (typed); a
   `SignalSource::JointForce(StableId)` if 2-1's joint-source pattern exists,
   otherwise leave the source to the Outcome.
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

*(worker fills in)*
