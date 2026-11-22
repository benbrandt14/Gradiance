# Context menu quick actions
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("loosen", "centre axle", "centre thruster", "transform into circle", "brake") · status: done

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

One-click actions on a body: drop all its joints, pin it to the world at
its centre, put a thruster at its centre, turn it into a circle of equal
area, and brake a hinge motor. Each is an operation, each is one undo step.

## Read first

- `crates/gradiance-ui/src/context_menu.rs` — the body section.
- `crates/gradiance-interaction/src/input.rs` — `KEY_OPS`, the id link.
- `docs/scripting.md` — the verb recipe.
- `crates/gradiance-domain/src/joint.rs:73` — `MotorConfig`.

## Facts

- Every action lowers to an existing intent: `delete-joint` (many),
  `spawn-joint` (world pin), `spawn-node` (thruster), `property-edit`
  (shape), joint property edit (motor). No new command type is needed.
- A multi-intent action must still be one undo step. *Unverified* that
  `OpOutput` batches several intents into one step; check before building.

## Approach

- Operations: `body.loosen`, `body.pin_centre`, `body.thruster_centre`,
  `body.to_circle`, `joint.brake` (motor on, target 0, cap kept).
- Context menu rows invoke them by id (`InvokeOp`), never the intent.
- Keep the rows as one table of operation ids by selection kind (as
  `ui::pie::TOOLS_PIE` is): the contextual task bar (B-2, 2026-11-21 plan)
  renders the same table, so add no per-row UI logic.

## Out of scope

New joint kinds. Gesture input.

## Done when

- Undo matrix covers all five operations.
- Test: `to_circle` keeps the area within 0.5 %.
- fmt, clippy, tests, doc gate green; `docs/ops.md` regenerated.

## Outcome

Done (2026-11-22).

- **Verified first:** `OpOutput` does not batch intents into one step;
  each intent is one command and the stack snapshots after each. So every
  action lowers to **one** intent. Loosen needed one change:
  `DeleteCommand` now also takes joint ids (a joint the body cascade
  already took is not despawned twice), so `body.loosen` is one
  `DeleteIntent` naming the body's joints.
- **Operations** (`command::ops::quick`): `body.loosen` (`(loosen i)`),
  `body.pin_centre` (a world-pin hinge at the body's origin),
  `body.thruster_centre` (the thruster tool's default thruster at the
  origin, pushing along local up), `body.to_circle` (equal area through
  `polygonize`, centred on the origin; circles and grounds are skipped),
  `joint.brake` (a hinge's motor on at 0 rad/s, the torque cap and damping
  kept; a hinge with no motor gets the default motor, auto cap; sliders are
  left alone). "Centre" is the body origin: a CSG body whose origin is off
  its centroid is pinned at the origin.
- `SceneSnapshot.joint_defs` (id and `JointDef`, id order) gives loosen its
  joints and brake its old value; `joint_links` is derived from it.
- **UI:** the context menu renders `BODY_QUICK_ACTIONS` (Loosen, Pin at
  centre, Thruster at centre, To circle, and S-5's Make gear) and, on a
  hinge, `HINGE_QUICK_ACTIONS` (Brake motor), as tables of operation ids
  invoked with `"selection"` or the joint id. A unit test pins that each
  id is registered and clickable.
- **Tests:** the undo matrix and the doorway conformance cover all five
  from their examples. The conformance fixture gained a box (body 5) so
  `to-circle` has a body to change. `to_circle` keeps the area of a box
  and a gear within 0.5 % (unit test).

