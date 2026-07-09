# Scripting spikes — findings

Status: **both linchpin spikes passed** (2026-07-08). Companion to
`docs/script-lisp-decision.md`, which gated any feature code on these two
experiments. Verdict: **proceed — the design's load-bearing assumptions hold,
and neither "rethink" branch is triggered.**

## Spike 2 — Tier-B driver kernel (perf) — PASS

Question: can driver expressions be evaluated at particle/fluid scale without
the scripting VM in the per-frame loop?

Landed in `src/script/kernel.rs` (pure, proptested). A numeric `Expr` tree
compiles once to a flat postfix tape; the hot path is a stack machine over a
fixed scratch array — no recursion, no dynamic dispatch, no heap — and `drive`
applies it across columnar SoA data with one reused variable buffer (zero
allocation per element).

- Correctness: proptested against a tree-walking oracle.
- Throughput (debug, opt-level 1): **~27.7 M evals/s** — 1M elements × 20
  frames of an 8-instruction `amp·sin(freq·t+phase)` in 723 ms.
- Reading: the structural claim (VM-free, alloc-free) is the result; the number
  confirms the ballpark, with release + SIMD + the GPU-portable flat-tape design
  as documented headroom. The particle/fluid ceiling is avian's object count,
  not the kernel.

## Spike 1 — bevy_reflect ↔ steel bridge — PASS

Question (the higher-risk linchpin): can ONE generic, reflection-driven bridge
make every `#[derive(Reflect)]` type scriptable with no per-field Rust code —
cheaply enough that "everything programmable" is a derive, not N builtins?

Validated first in an isolated crate (fast iteration), then in-repo against
gradiance's **real** `SimSettings` — the integration test `tests/script_spike.rs`
on branch `claude/script-reflect-steel-spike`, with steel as a dev-dependency so
nothing is committed to the shipping build (or to `main`'s build cost) yet.

**Result: clean.** A steel script mutates a real Rust value entirely through
reflect paths — `speed` (f32), nested `gravity.y` (glam `Vec2`), `substeps`
(u32 via numeric coercion) — leaving untouched fields intact, and the whole
value round-trips to steel data for the "reads are total" path:
`(("gravity" (("x" 0.0) ("y" -500.0))) ("speed" 2.0) ("substeps" 12))`. The
Rust bridge (`read_path`/`write_path`/`reflect_to_steel`) **never names a
field** — it is purely reflection-driven, so it generalizes to any Reflect type.

Opaque custom types (the `ShapeDef`-as-handle path for geometry constructors)
also round-trip: `impl steel::rvals::Custom for T {}` is legal for user types
(`Sealed` is blanket-impl'd for `T: Any`), and `(shape-radius (make-circle
7.0)) => 7.0` confirms a Rust value carried through steel and back.

### steel viability

- On crates.io as `steel-core` v0.8.2 (lib name `steel`), plus `steel-derive`,
  `steel-interpreter`. Exact-pinnable, consistent with the repo's `=`-pin
  discipline.
- Standalone build ~1 min; dependency tree is moderate (im-lists, bincode,
  arc-swap, crossbeam, inventory, which, xdg, …). Because steel is the
  *authoring-time* (Tier A) engine and never runs in the per-frame loop, this
  weight stays off the hot path — the two-tier rule is what makes a heavy
  engine acceptable.
- `register_fn` requires `Fn(..) -> R: IntoSteelVal` with `Send + Sync +
  'static`, so closures capturing `Arc<Mutex<..>>` work (the seam-bound
  builtin pattern). `run` returns `Result<Vec<SteelVal>, _>`.

### API gotchas (captured so P0 doesn't rediscover them)

- **`dyn PartialReflect` does not satisfy the `GetPath` blanket impl.** Keep
  reflect-path helpers generic over the concrete `T: Reflect`; erase to
  `&dyn PartialReflect` only for the structural walk (`reflect_ref`,
  `try_as_reflect`), which works on trait objects.
- **`Struct` is at `bevy_reflect::structs::Struct`** (not root-re-exported);
  `ReflectRef`/`GetPath` are root-exported. Trait-object methods on
  `ReflectRef::Struct(&dyn Struct)` need no import.
- **Leaf writes:** "try each concrete `try_apply`" (f32/f64/u32/i32/bool) is a
  clean, TypeId-free way to coerce a scalar onto an unknown leaf type;
  `try_apply` checks type first, so failed attempts don't mutate.
- **steel is Scheme:** avoid builtin names that collide with special forms
  (`set!`); and integer literals arrive as `IntV`, not `NumV`, so numeric
  setters should accept `SteelVal` and coerce rather than bind `f64` directly.

## What the spikes did *not* cover (P0 scope)

- Dispatching through the **intent** seam (vs. the settings-resource seam shown
  here) inside a real `&mut World` exclusive system — the same bridge, a
  different sink. Straightforward given the above; it is P0 wiring, not a risk.
- The operation **registry** enumerating `Reflect`-derived ops for
  discoverability (`(ops)`, `(describe …)`).
- Promoting the bridge from `tests/` into `src/script/` as product code, with
  steel a feature-gated real dependency, `ScriptError` (thiserror) in place of
  test-only `expect`, a fuel/step budget, and a `catch_unwind` boundary.
- A `tests/boundaries.rs` rule confining `steel` to `src/script/` — added when
  steel becomes a product dependency (nothing to confine while it is dev-only).

## Recommendation

Adopt steel and proceed to P0 as scoped in the decision record. The bridge is
low-boilerplate exactly as hoped: one converter, and every `Reflect` type is
scriptable. The only standing cost is build weight, which the two-tier
architecture already quarantines to the authoring path.

## Spike #1 follow-through — `StableId`/`ShapeDef` reflect-opacity (RESOLVED 2026-07-09)

Spike 1 proved the bridge on a *settings resource* but deferred the harder
question that actually blocks the operation registry: **how do the authored
intents reflect?** `SpawnBodyIntent`/`SpawnJointIntent` reach `StableId` (a
`Uuid` newtype) and `ShapeDef` (an SDF enum still in flux) — neither of which
derived `Reflect` — so the registry could not yet bind body/joint ops by name.
`CutIntent` (all-`Vec2`/`f32` leaves) was the only reflected intent. Resolution:

- **Both reflect as opaque** via `#[reflect(opaque)]`. The attribute needs only
  `Clone` (both satisfy it) and auto-derives `FromReflect`, so a value round-trips
  by clone. Opacity is the *correct* model, not a workaround:
  - `StableId` is an identity **handle**, never authored byte-by-byte through
    reflection; opacity also keeps the reflect graph off bevy_reflect's optional
    `uuid` feature.
  - `ShapeDef` is the ratified **opaque foreign value** (`script-lisp-decision.md`):
    built by geometry constructor builtins, passed as a handle. Opacity insulates
    the scripting surface from the enum's churn; script-side tree-walking is
    additive later via accessor builtins.
- **Everything else reflects structurally** — no further opacity was needed:
  `PosRot`, `Rgba`, `Appearance`, `LayerMask32`, `MotorDef`, `JointCommon`,
  `JointKind`, `JointDef`, `BodyRecord`/`JointRecord`/`EnvironmentRecord`/
  `SceneRecord`, `PropertyValue`/`PropertyChange`, `ArrayMode`, `TransformChange`.
  Crucially, **`BodyPhysics` reflects over avian's own components**: avian derives
  `Reflect` on `RigidBody`/`Friction`/`Restitution`/`ColliderDensity`/
  `GravityScale` unconditionally (it turns on bevy's `bevy_reflect` feature), and
  with our `serialize` feature they even carry `reflect(Serialize, Deserialize)`.
  The de-adapter collapse thus *helps* introspection exactly as predicted — the
  read-total path reads authored physics through reflection over avian directly.
- **The whole authored intent surface now derives `Reflect` and is registered**
  in `CommandPlugin`. `App::register_type` recursively registers each intent's
  field types, so one call per intent gives the read-total path a complete
  registry (records → opaque handles → domain types → avian components) without
  naming those types individually.

Verified **feature-independently** by `tests/reflect_intents.rs` (runs in normal
`cargo test`, no `--features script`): `ReflectRef::Opaque` for
`StableId`/`ShapeDef` (incl. a deep CSG tree as one leaf); top-level + transitive
registration (down to avian `RigidBody`/`Friction`); and a `to_dynamic()` →
`from_reflect()` round-trip for `SpawnBodyIntent`/`SpawnJointIntent` that
reconstructs the record intact across the opaque leaves.

**API notes (so P1 doesn't rediscover them):**
- `#[reflect(opaque)]` forgoes `Struct`/`Enum` (`reflect_ref()` → `Opaque`) and
  requires `Clone`; `FromReflect` is auto-provided unless `from_reflect = false`.
- Deriving `Reflect` on a struct requires each non-ignored field to be
  `FromReflect + TypePath` — opaque leaves satisfy this, which is why records
  embedding `StableId`/`ShapeDef` derive cleanly.
- `PartialReflect::to_dynamic()` (bevy 0.19; the former `clone_value`) →
  `FromReflect::from_reflect(&dyn PartialReflect)` is the round-trip; opaque
  fields survive it by clone.

**Unblocked next (P1):** the emit-then-dispatch operation registry can now
construct these intents from steel data via the generic bridge and enumerate
them by reflected type name — the World-integration constraint (scripts emit
intent values; one exclusive system dispatches through the intent bus) is
unchanged.
