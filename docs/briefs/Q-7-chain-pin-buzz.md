# Chain link buzz next to a world pin
Phase Q (follow-ups) · N-5 Outcome, "Learned": "a hanging chain at rest
keeps a small solver buzz in one link next to a world pin (about 0.4 m/s at
its centre)" · status: returned

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

A chain pinned to the world at both ends comes to rest: after it settles,
no link moves faster than 0.02 m/s (today one link next to a pin keeps
about 0.4 m/s, steady, with linear drag on). The cause is found and fixed
at its source, with a test that fails on `523ed82`. If the cause is inside
avian and cannot be fixed from Gradiance, the Outcome says so with the
evidence, the smallest repro is kept as an `#[ignore]`d test with the
reason, and an upstream note goes into `docs/upstream-watch.md`.

This is an investigation brief: stop and hand back if the cause turns out
to need a change to the joint model (for example a new joint kind), and
write what you found into the Outcome.

## Read first

1. `docs/physics-deadapter-decision.md` (avian is used directly; derived
   joint entities).
2. `crates/gradiance-physics/src/joint_sync.rs` (`sync_joints`, the world
   pin anchor, `retune_joints`).
3. `crates/gradiance-command/src/ops/chain.rs` (what a chain is made of).
4. `tests/it/chains.rs:311-400` (the catenary and swing tests).

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- A world pin is a derived `RigidBody::Static` entity with no collider at
  the anchor point, spawned as a `ChildOf` the joint entity
  (`joint_sync.rs:99-109`); the joint then runs from the pin (local anchor
  zero) to the body.
- Chain links are capsules of density 5 (`CHAIN_DENSITY`,
  `crates/gradiance-command/src/ops/chain.rs:35`, applied at `:196`), 25 cm
  by 6 cm by default; consecutive links are hinged at their shared end
  point, and only directly jointed pairs ignore each other's collisions
  (decision `shared-pivot-collisions`, J-1).
- Islands and sleeping are disabled
  (`crates/gradiance-physics/src/lib.rs:38`), so nothing puts a settled
  chain to sleep.
- Substeps are a `SimSettings` field (`crates/gradiance-domain/src/settings.rs:380-382`);
  N-5 reports that more substeps did not remove the buzz.
- The catenary test tolerates the buzz: it checks the sag and the mean
  link speed under 0.1 m/s (`tests/it/chains.rs:349-356`), not the
  fastest link.

## Approach

1. Reproduce headless: the N-5 catenary scene, record each link's speed
   and the hinge gaps over the last second; find the buzzing link and
   whether it is the first link at a pin, or any link next to a pin.
2. Test the candidate causes one at a time, each with a measurement in the
   Outcome:
   - the pin's static anchor as a child of the joint entity (its
     `Position` from a propagated transform each frame) against a
     top-level static entity;
   - link i and link i+2 touching near a pin (contacts that the shared
     pivot rule leaves on), seen with the 6-3 contact overlay or the
     contact count signal;
   - the hinge's angular or point compliance and `JointDamping`;
   - warm starting / the joint solve order of the pin joint against the
     link joints.
3. Fix at the source (Gradiance side), or document the avian cause as
   above.

## Tests

- `tests/it/chains.rs`: `a_settled_chain_has_no_buzzing_link` (the N-5
  scene; after 10 s the fastest link is under 0.02 m/s over the last
  second). It fails on `523ed82`.
- The existing chain, joint stress (J-1) and joint tests pass.
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-physics`, the
  touched `--test it` modules.

## Out of scope

- Turning sleeping on (a separate decision with its own risks for every
  scene).
- New joint kinds, rope, slack chains.
- Changing the shared-pivot collision rule, unless the investigation shows
  it is the cause; then record a decision in `archive.ron` before the
  change.

## Outcome

Returned (2026-11-21): the cause is inside avian; documented, not fixed.

Measurements (the N-5 catenary scene, 20 links, linear drag 10, after
10 s):
- The buzzing link is the **first link at each world pin**: 0.44 m/s at
  the left pin, 0.08 m/s at the right. Every other link reads about
  0.022 m/s.
- Its pose is **frozen**: position constant to 1e-7 m over 60 frames, yet
  it reports a steady `LinearVelocity` (−0.357, −0.257) and
  `AngularVelocity` 1.169 rad/s. The pin hinge stays stretched **1.5 mm**
  under the hanging chain's weight. This is a standing joint correction
  that avian reports as velocity, not motion.
- Ruled out: air drag (drag set to 0 after settling: 0.455 m/s), sleeping
  (`Sleeping` absent), contacts (`CollidingEntities` empty), the pin as
  `ChildOf` the joint entity (top-level static pin: identical numbers).
  N-5 already reported that more substeps do not help.

Kept: `tests/it/chains.rs` `a_settled_chain_has_no_buzzing_link`,
`#[ignore]` with the reason. Upstream note in `docs/upstream-watch.md`
§ "Dependency notes". Follow-up for the planner: readouts and plots of
link speed show the phantom value. A fix would need either avian's joint
solve to report relaxed velocities, or a Gradiance-side filter of reported
speed, which would be a decision.
