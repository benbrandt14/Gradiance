# Decision: collapse the avian adapter (burn the rapier boat)

Status: **accepted + complete** (2026-07-09). Retires the engine-swap seam
(invariant #3). Supersedes the "engine-agnostic domain + `physics::queries`
facade" adapter with **direct, idiomatic avian usage where physics is actually
done**, while keeping the authored/derived split (#5) and the command/intent
discipline (#1–#2).

> **Completion note (2026-07-09).** The collapse is done: `PhysicalProps` and its
> per-frame translation are gone (`BodyPhysics` is now a capture/undo value object
> over the authored avian components), the read facade returns avian types, and
> `FORMAT_VERSION` is at 2. The one increment this document **over-scoped** was
> joints/motors (increment 3): they were already avian-native and required **no
> code change** — see the corrected §3 below.

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
> **Correction (2026-07-09): joints were never part of the collapse.** The three
> bullets originally here over-scoped the work — joints have been avian-native
> since M6 and needed **no change**:
>
> - `domain::joint::{JointDef, MotorDef}` are **not mirrors**; they are the thin,
>   `StableId`-keyed authored layer that is *genuinely necessary* (avian joints
>   hold a raw `Entity`, which invariant-#5/identity rules forbid persisting).
>   They are already avian-shaped (a near-1:1 description of kind + anchors +
>   limits + native-motor params).
> - `physics::joint_sync` was **already a direct constructor**, not a translation:
>   it resolves `StableId` → `Entity` and builds `RevoluteJoint`/`FixedJoint`/
>   `PrismaticJoint` with native `AngularMotor`/`LinearMotor` directly.
> - `physics::motor.rs` is **not a hand-rolled velocity controller** and is
>   **kept, not deleted**: avian owns velocity tracking / max-force / damping; the
>   file is a ~90-line feature that only flips a native motor's target velocity at
>   the joint limits (the Algodoo "oscillate" behavior). Deleting it would delete
>   a feature, not adapter cruft.

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
3. **Joints/constraints/motors (the core) — ALREADY SATISFIED, no code change.**
   This increment was over-scoped: the desired end-state already existed as of
   M6. The joint record is already thin and avian-shaped, `joint_sync` is already
   a direct constructor emitting native motors, and `motor.rs` is a thin
   oscillate *feature* (kept, not deleted — see the correction above). Nothing to
   migrate; the joint inspector, joint commands, and `tests/joints.rs` /
   `tests/joint_edit.rs` were already exercising the avian-native path and stay
   green as-is.
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
