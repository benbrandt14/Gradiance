# Authored initial velocity and spin
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("set linear and angular velocity") · status: done

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A person sets a body's starting velocity and spin in the inspector (with a
direction dial like gravity's) or by script, `(set-velocity i vx vy)` and
`(set-spin i w)`. When a run starts, each body starts with that motion.
Undo of the run restores it. The value saves with the scene.

## Read first

- `crates/gradiance-physics/src/fields.rs:200` — `SetOrbitRequest`, the
  one-shot velocity write today, and why it is not undoable.
- `crates/gradiance-command/src/lib.rs:190` — how a run becomes one undo step.
- `crates/gradiance-domain/src/fluid.rs` + brief N-7 Outcome — the newest
  example of an optional authored body component end to end (record,
  `PropertyValue`, cut inheritance, snapshot).
- `docs/scripting.md` — the verb recipe.

## Facts

- Velocity is not authored: `LinearVelocity`/`AngularVelocity` are not in
  the save content (CLAUDE.md invariant 5; `gradiance-scene` has no
  velocity field, checked 2026-11-20).
- The settings window already has a direction dial for gravity
  (`ui/settings.rs:289`). Reuse it.

## Approach

- New authored `domain::motion::InitialMotion { velocity: Velocity2, spin:
  AngularVelocity }`, optional on a body (absent = at rest). `BodyRecord`
  field with a serde default, `PropertyValue::InitialMotion`, cut pieces
  inherit it, mirror flips it with the shape.
- One system applies it to avian's velocities on the frame a run opens
  (paused → playing from an edit state), never on resume within a run.
- `body.set_velocity`, `body.set_spin` operations (edit seam, one undo step).
- Inspector "Motion" section: magnitude + direction dial, spin field
  (`widgets::precise_drag`).
- "Set in orbit" may write `InitialMotion` instead of a one-shot when paused.
  Worker decides and records it.

## Out of scope

Per-body vector arrows (S-2). Velocity keyframes or drivers.

## Done when

- Behaviour test: a body with `velocity (3, 0)` on a frictionless floor has
  moved 3 m ± 2 % after 1 s of play; undo restores pose and `InitialMotion`.
- Undo matrix (`tests/it/undo_matrix.rs`) covers both new operations.
- fmt, clippy, tests, doc gate green. `docs/ops.md` regenerated.

## Outcome

Done (2026-11-21, branch `claude/jolly-noether-su0a6h`).

- `domain::motion::InitialMotion { velocity: Velocity2, spin: AngularVelocity }`,
  optional on a body. `BodyRecord.motion` (`serde(default)`, skipped when
  `None`, so older files and goldens are unchanged; no `FORMAT_VERSION`
  bump). Also `PropertyValue::InitialMotion` and the snapshot's `motion`.
  A cut piece inherits it. A mirror reflects the velocity across the mirror
  line and reverses the spin (`ScaleCommand`, `InitialMotion::mirrored`).
  A motion at rest is stored as none.
- **When it applies.** On `OnExit(Paused)` unless the newest undo step is
  the `simulate` step that pause just recorded (a resume keeps the live
  motion), and whenever a body's `InitialMotion` is added or changed while
  playing (a scene loaded mid-play, `gradiance run`, an edit made in
  play), through `start_new_motion`. Dynamic bodies only.
- **Operations.** `body.set_velocity` `(set-velocity i vx vy)` and
  `body.set_spin` `(set-spin i w)`: edit seam, one `property-edit` step
  each, picked up by the undo matrix from their examples.
- **Inspector.** A "Starting motion" row has the gravity dial (now
  `settings::direction_dial`, which also reports the end of a gesture so a
  dial drag commits once), plus speed, direction and spin fields on the
  edit seam.
- **"Set in orbit" stays a one-shot** (session decision): it sets the live
  velocity for the orbit that exists now, which is the Algodoo
  behaviour; authoring it as a start condition would change what undo of
  the menu row means. Revisit if someone wants orbits to replay.
- Tests: `tests/it/initial_motion.rs` (3 m ± 2 % after 1 s at 3 m/s;
  undo restores pose and motion; a resume keeps the live motion; spin,
  rest, save and mirror) and a domain unit test. The persistence
  round-trip proptest now covers the field.
- Checked on lavapipe: the section reads 2.50 m/s at 36.870° with spin
  68.75°/s, and play launches the box with that motion.
- Learned: two remote calls in one frame lower against the same snapshot,
  so `set-spin` then `set-velocity` without a frame between them loses the
  spin. This is the documented one-snapshot rule, not a bug, but worth
  knowing when scripting the pair.

