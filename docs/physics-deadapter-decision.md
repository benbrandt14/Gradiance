# Decision: collapse the avian adapter (burn the rapier boat)

Status: **accepted** (2026-07-09). Retires the engine-swap seam (invariant #3).
Supersedes the "engine-agnostic domain + `physics::queries` facade" adapter with
**direct, idiomatic avian usage where physics is actually done**, while keeping
the authored/derived split (#5) and the command/intent discipline (#1–#2).

## Why

The multibody/constraint model — bodies, joints, constraint trees, and the
operations on them — is the core of the tool. The adapter (mirror components +
`Changed<>` translation systems + a hand-rolled motor controller) put framework
cruft between the *description* of a multibody and the *operations* on it. This
project is single-maintainer and prizes legible, introspectable, scriptable
code over engine-swappability. On the record: **avian updates breaking save
files is acceptable.** So we collapse to avian types.

## What collapses (mirror + translation → gone)

- **`domain::props::PhysicalProps` + `physics::body_sync` (the translation).**
  Authored physical state becomes avian's own components on the entity:
  `RigidBody`, `Friction`, `Restitution`, `ColliderDensity`, `GravityScale`,
  `Sensor` (presence), `LockedAxes`. No per-frame prop sync — avian owns them.
  `body_sync` shrinks to the one genuine derivation it always was: **`ShapeDef`
  → `Collider`** (via `geometry::polygonize`).
- **`domain::joint::{JointDef, MotorDef}` mirrors → thin, avian-shaped.** Joint
  authoring maps 1:1 onto avian joint kinds (`RevoluteJoint`, `PrismaticJoint`,
  `FixedJoint`) and **native motors** (`AngularMotor`/`LinearMotor` +
  `MotorModel`).
- **`physics::motor.rs` (hand-rolled velocity controller) → deleted.** Replaced
  by avian native motors (the joint doc already said "prefer native").
- **`physics::joint_sync` translation → a direct constructor.** Its remaining,
  necessary job: resolve `StableId` → `Entity` and build the avian joint.

## What stays (thin, because it is genuinely necessary)

- **Joint authored form** — a thin record that references bodies by `StableId`
  (never a raw `Entity`) and carries kind + anchors + limits + native-motor
  params. This is the Q1 "thin layer definitely necessary" case: avian joints
  hold `Entity`, which must never be persisted. It stays *avian-shaped* (a near
  1:1 description), not an abstraction.
- **`LayerMask32`** — carries the project's layer-bit → extrusion-z-depth
  semantics, which avian's `CollisionLayers` does not model. Maps to avian via
  `CollisionLayers::from_bits` on the derived side.
- **`ShapeDef`, `StableId`, `Appearance`, geometry** — never were avian; unchanged.
- **The snapshot/persist + command/intent machinery** — the save/undo mechanism,
  not adapter cruft. Records now carry authored avian components directly.

## Persistence

Enable avian's **`serialize`** feature. `BodyRecord` carries the authored avian
components directly (they are `Serialize`); the joint record stays `StableId`-based
and reconstructs the avian joint on spawn. Save-file stability across avian
versions is explicitly *not* a goal (accepted above). `FORMAT_VERSION` bumps.

## Read facade (Q2)

Keep a **thin `physics::queries` that now returns avian types** (velocities,
spatial hits, sleeping state) — one testable, discoverable read cut-point shared
by interaction, UI, render debug, and the scripting reflection bridge. It is a
convenience/DRY layer, no longer an abstraction boundary. Wrapping that does not
earn its keep (pure component reads) is dropped; consumers may read avian
components directly. The scripting "reads are total" path reads avian components
through reflection — the collapse *helps* introspection.

## Invariant changes (`CLAUDE.md`)

- **#3 (avian only in `src/physics/`) — retired.** Replaced by: *avian is used
  directly wherever physics is done; authored physics state is avian components;
  identity is still `StableId` and raw `Entity` is never persisted or
  cross-referenced.*
- **#5 (authored vs derived) — kept**, reworded: the authored set now includes
  the avian components listed above (they are the save file); derived state
  (`Collider`, `Mass`, contacts, `Position`/`Rotation` from `Transform`, meshes,
  live joint entities' internal state) is still rebuilt and never serialized.
- **#1–#2 (command/intent choke point) — unchanged.** All mutation still flows
  through intents → dispatch. Scripting still emits intents (Spike 1 finding).
- `tests/boundaries.rs`: **drop the avian-confinement test**; **keep** the steel
  rule and the serialize-confinement rule (the latter guards *our* serde derives;
  avian's are external). Add a rule that raw `Entity` is not stored in authored
  records if cheap to express.

## Sequencing (each increment lands fmt+clippy+test green)

1. **Props collapse.** Enable `serialize`; replace `PhysicalProps` with authored
   avian components; `body_sync` → collider-only derivation; update `BodyRecord`
   capture/spawn, `SpawnBodyCommand`, `property.rs`, inspector, `FORMAT_VERSION`.
2. **Read facade.** Thin `physics::queries` to return avian types; update the ~7
   consumers; delete now-pointless wrapping.
3. **Joints/constraints/motors (the core).** Thin avian-shaped joint record;
   `joint_sync` → direct constructor with native motors; delete `motor.rs`;
   update joint inspector, joint commands, joint tests.
4. **Invariants + boundaries + docs.** Rewrite #3/#5 in `CLAUDE.md`; drop the
   avian boundary test; refresh `docs/architecture.md` physics section.

Testability is a first-class constraint throughout (Q2): the headless joint/
motor tests (`tests/joints.rs`, `tests/joint_edit.rs`) are the regression net —
they must stay green as the constraint path becomes avian-native.

## Interaction with scripting (the overlap)

No scripting decision changes. The reflection bridge reflects avian components
directly — the collapse *removes* the facade indirection it would otherwise read
through, which is strictly simpler. The World-integration constraint (Spike 1:
scripts emit intents, they cannot hold `&mut World`) is unaffected.
