# Scripting spikes — findings (condensed)

Status: **both linchpin spikes passed** (2026-07-08); the P0/P1 work they
gated has landed (`src/script/`). This is the durable record — verdicts,
numbers, and the API gotchas worth keeping. Companion to
`docs/script-lisp-decision.md`.

## Spike 2 — Tier-B driver kernel (perf) — PASS

`src/script/kernel.rs`: numeric `Expr` tree → flat postfix tape → stack
machine over a fixed scratch array (no recursion, dispatch, or heap; zero
allocation per element). Proptested against a tree-walking oracle.
Throughput: **~27.7 M evals/s** debug at opt-level 1 (1M elements × 20 frames
of an 8-instruction expression). The particle/fluid ceiling is avian's object
count, not the kernel.

## Spike 1 — bevy_reflect ↔ steel bridge — PASS

One generic converter (`src/script/reflect_bridge.rs`) reads/writes any
`#[derive(Reflect)]` value by reflect-path — no per-field Rust code, so
"everything programmable" is a derive, not N builtins. steel (`steel-core`
=0.8.2) is exact-pinnable; its build weight is quarantined to the Tier-A
authoring path by the two-tier rule.

### Reflect-opacity resolution (2026-07-09)

`StableId` and `ShapeDef` reflect as **opaque** (`#[reflect(opaque)]`) —
identity handle and foreign geometry value respectively; everything else in
the authored intent surface reflects structurally, including `BodyPhysics`
over avian's own `Reflect`-deriving components. All intents derive `Reflect`
and are registered in `CommandPlugin` (`register_type` pulls transitive field
types). Verified by `tests/it/reflect_intents.rs`.

### API gotchas (kept so future work doesn't rediscover them)

- `dyn PartialReflect` doesn't satisfy the `GetPath` blanket impl — keep
  reflect-path helpers generic over `T: Reflect`; erase only for the
  structural walk.
- Leaf writes: try each concrete `try_apply` (f32/f64/u32/i32/bool);
  `try_apply` checks type first, so failed attempts don't mutate.
- steel is Scheme: avoid builtin names colliding with special forms (`set!`);
  integer literals arrive as `IntV`, so numeric builtins take `SteelVal` and
  coerce (`steel_to_f64`).
- `#[reflect(opaque)]` requires `Clone`, auto-provides `FromReflect`;
  `to_dynamic()` → `from_reflect()` round-trips opaque leaves by clone.
