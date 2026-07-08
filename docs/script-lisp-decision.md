# Decision: a Lisp scripting layer as the tool's governed control plane

Status: **accepted (direction)** (2026-07-08). Encodes the design intent for
the scripting / symbolic-modeling feature. Scope of this document is the
*architecture and the decisions*, not the implementation — no code lands until
the two linchpin spikes (below) are done.

## Question

Not "should Gradiance have a scripting console" (it should — the roadmap
already lists *symbolic & equation input* and *scripting* under backlog). The
question is: **what is the smallest set of architectural commitments that lets
a Lisp/DSL make the _whole tool_ programmable — geometry, scene, editor
config, engine internals, and eventually tools themselves — without becoming a
bypass around the command discipline, without a second on-disk representation
to maintain, and without capping the runtime at a scale that precludes
particle/fluid workflows?**

Four things must be true at once, and they pull against each other:

1. **Expressive and homoiconic** — the shape *is* the code; every operation the
   tool can perform has a data form the language can construct and inspect.
2. **Governed** — scripting cannot violate the invariants in `CLAUDE.md`
   (all world mutation through commands; tools/UI never mutate directly;
   `avian2d` only in `src/physics/`; `egui` only in `src/ui/`; authored vs
   derived).
3. **Maintainable and cohesive** — built canonically *alongside* every other
   feature, never a side-car that decays into an unmaintained path; one save
   format, so two representations can never diverge.
4. **Fast at scale** — driving thousands of objects per frame must not pay a
   scripting-VM tax per object, or particle/fluid simulation is off the table.

## Verdict

Adopt a **Lisp front-end over a governed, homoiconic operation registry**, with
a **two-tier execution model** that keeps the scripting VM strictly out of the
per-frame inner loop. The registry is a **runtime** construct — it powers
scripting, undo, and the future node editor through the *same* intent seam tools
and UI already use, so it cannot become a side-car that rots on its own.
Persistence stays **RON of materialized authored state as the single on-disk
representation** — no second, divergable save format (an optional parametric
scene *script* is a user-authored source that produces state, not a competing
save). Modeling is **imperative + physics-solver** now; a constraint solver is a
deferred consumer behind a declarative seam. This is an
**accrete-through-milestones** feature, not a standalone project: each upcoming
milestone is built *through* the registry seam so programmability becomes true
by the time we arrive.

## The four forks, resolved

| Fork | Decision | Rationale |
|---|---|---|
| **Scene model** | **RON of materialized state stays canonical and is the only on-disk format; the operation registry is runtime-only** | Prioritizes maintainability over representational purity: exactly one save format, so two representations can never diverge. The homoiconic registry still gives scripting / undo / node-editor their shared runtime seam; parametric replay, if ever wanted, is an *optional authored script* that regenerates state — not a second canonical save. |
| **Modeling style** | **Imperative + physics solver (floor); constraints deferred behind a declarative interface** | A constraint solver is real subsystem risk and Rust's 2D-constraint ecosystem is thin. Keep it a named escalation path (CAD kernel / Wolfram / `argmin`), not a foundation. "Expose avian internals that make sense" = the reflect registry, grown incrementally. |
| **Experiments** | **Live probes / tracers / plots; no headless batch — but don't preclude it** | Keep scope tight. Deterministic stepping + a data-out (`measure`) seam are designed now so a batch runner is additive later. |
| **Units / frames** | **Raw engine-native numbers now; values shaped so unit-typing is additive** | Lightest weight wins today; a tagged-quantity layer can wrap numbers later without rewriting scripts. Frames named explicitly at each builtin. |

## The spine: a governed, homoiconic operation registry

In an ECS all state is uniform data (components + resources) and `bevy_reflect`
makes it addressable by name at runtime (already used in
`src/ui/reflect_grid.rs`). That is the enabler for "the whole tool is
programmable." The governance rule that makes it coexist with the invariants:

- **Reads are total and free.** A script may reflect or query *any* component
  or resource. Read-only cannot violate an invariant.
- **Writes are never direct.** A script never sets a reflected field. It
  invokes a **registered operation**, which routes to the correct existing
  seam:
  - authored-world verbs (`spawn`, `set-friction`, `orient`, `cut`) → **emit
    intents** (undoable) — the existing choke point;
  - config verbs (`set-gravity`, `set-grid-basis`, `set-default-layers`) →
    **write the settings resource** — the sanctioned invariant-#4 exception
    (gravity orientation *is* `SimSettings`, grid basis *is* `GridSettings`);
  - editor-state verbs (`select`, `set-active-tool`, `focus-camera`) →
    editor-state resources.

This read-total / write-mediated asymmetry is exactly what the UI already does
(reads component copies, emits intents), generalized to the whole surface. It
stays **CI-enforceable**: `tests/boundaries.rs` gains a rule that the operation
registry may *dispatch* through intents / settings-writes but may never
`get_mut` an authored component — the registry must not become a second,
ungoverned mutation path.

### Homoiconic intents — one runtime representation, many front-ends

If every intent has a data form (`(op-name arg…)`, backed by `Reflect`), one
*runtime* representation unifies everything that mutates the tool: the REPL
types it, the future node editor emits it, a scripted tool returns it, a macro
is a list of them. This is the shared seam — the same doorway tools and UI
already use — which is exactly why the scripting path cannot rot independently
of the editor (see *Cohesion* below).

Persistence is deliberately *not* this log. The canonical save stays RON of
materialized authored state (today's format, zero new persistence code). If
parametric/replayable scenes are ever wanted, the intent sequence can be
*exported* as an optional authored script (`.scm`) that regenerates a scene — a
source artifact, like a build script, not a second representation of saved
state. Undo inverses stay derived at apply time exactly as today, in-memory,
never serialized (rule #5).

## The perf spine: two tiers, one language

The single most important guardrail. **The scripting VM is never in the
per-frame inner loop.**

- **Tier A — cold / authoring (steel VM).** REPL, scene-building ops, macro
  expansion, and *compiling* drivers. Runs on user actions, not per frame.
  steel's weight is therefore irrelevant to runtime cost — which is what
  justifies choosing a heavy, ecosystem-rich engine.
- **Tier B — hot / runtime (compiled numeric kernels).** The DSL-subset lowered
  to a flat, allocation-free evaluable (register-VM or a closure-tree of
  `fn(&[f32]) -> f32`) that runs over a query/buffer **in one Rust system** —
  data-parallel, SIMD-friendly, GPU-portable later. No VM, no per-element
  allocation, no dynamic dispatch in the loop.

Corollaries that protect particle/fluid scale:

- **Bulk runtime updates never touch the intent/command/undo path.** We log
  that *"a fluid emitter exists with these params"* (one authored op); we never
  log or command-wrap each particle's per-frame motion (derived, never
  recorded). Pushing a command per particle per frame is the perf disaster this
  rule forbids.
- **Drivers are kernels over sets, not closures per entity.** You script the
  *system*, not the particle — the driver abstraction is "a kernel over a
  query," which is how it stays cheap and how it maps onto both an ECS system
  and (later) a compute shader.
- **High-count populations are a derived bulk buffer, not per-particle authored
  bodies.** The domain model must not assume every simulated thing is a
  `StableId`'d authored body with an avian collider. Particles/fluid are a
  separate, derived, bulk-simulated population (SoA arrays, possibly a dedicated
  SPH/PIC-FLIP solver later, possibly GPU). The scripting layer adds ~zero
  overhead to objects it does not drive; the object-count cap is avian's, not
  scripting's.

## The three mutation categories (keep them distinct)

1. **Edits** — discrete, human-scale, **undoable**, go through intents. "Build/
   modify the scene."
2. **Drivers** — continuous, per-frame, **derived, never recorded**, evaluated
   by a Tier-B kernel seam (the motor pattern generalized: expression is
   authored, evaluation is derived — rule #5). "This body's x is `sin(t)`."
3. **Sim-events** — mutations *during* simulation (spawn-on-trigger). Neither an
   authored edit nor a pure driver. A **deliberately non-undoable** category so
   it never corrupts undo. Named now so it is not discovered by accident later;
   its full design is explicitly deferred past the first spike.

## Drivers as a named-signal dataflow environment (node-editor substrate)

The future Simulink-style node editor must be "purely an editor." That forces
one choice now: model drivers as **a set of named bindings**, not isolated
per-actuator expressions.

- named binding (`(defsignal error (- target (sensor-angle j1)))`) = a node;
- symbol reference = a wire;
- shared subexpression = fan-out;
- feedback loop = a read against *last frame's* state (also how eval-order
  cycles are broken).

Built this way, the node editor authors the same signal-binding data scripts
author, emits it through the same intents, and needs zero runtime of its own.
Algebraic loops: the scheduler topo-sorts and breaks cycles via last-frame
reads, with cycle-detection warnings (hard cases deferred, named here).

## Cohesion & long-term enablement guarantees

This addon must be built *canonically alongside* every other feature, never as a
side-car that decays into an unmaintained path. Three structural guarantees make
that true — they are the answer to "will this rot?" and to "are the node UI and
live plotters really enabled by this foundation?":

- **One doorway, no side-car.** Scripting introduces *no* new mutation path. It
  emits the same intents through the same registry that tools and UI already
  use. If the editor's normal edit path keeps working, scripting keeps working —
  they share the seam, so they are maintained together by construction. The
  `tests/boundaries.rs` rule (registry may only dispatch through intents /
  settings-writes) keeps this honest under CI.
- **Node-driven UI is enabled by the dataflow substrate.** Because drivers are
  modeled as named signals (nodes) and symbol references (wires) — not anonymous
  per-actuator expressions — the future node editor is *purely an editor*: it
  authors the same signal-binding data, emits the same intents, and needs no
  runtime of its own. This is a design constraint on P2, not a later retrofit.
- **Live plotters are enabled by read-total governance.** Introspecting physics
  state to plot it is a pure *read*, which the governance model already makes
  total and free. The only foundation they need is a stable read facade over
  simulation/derived state — the same `physics::queries`-style seam scripts read
  through. Plotters are therefore just another reader: no new mutation, no new
  persistence, no exception to any invariant. Keeping that read facade complete
  and stable *is* the enablement.

Net: both long-term concepts — a node-driven control UI and live
physics-introspecting plotters — are foundationally enabled by the two halves of
the same governance model (write-mediated registry, read-total facade). Neither
is a special case bolted on later.

## Scene model detail: one canonical format

- **RON of materialized authored state = the only on-disk representation**,
  exactly as today. No second save format means two representations can never
  drift — the maintenance concern that drove this choice.
- **Operation registry = runtime only.** It backs scripting, undo, and the node
  editor; it is never a persisted parallel to RON.
- **Optional parametric export** (later, if wanted): serialize an intent
  sequence as an authored `.scm` script that *regenerates* a scene. This is a
  source file the user owns (like a build script), categorically different from
  a save — so it introduces no divergence.
- **Bake** = an explicit operation that freezes simulated/derived state into
  authored state (e.g. "keep these settled positions"), so simulated results
  only ever enter the save deliberately.

Deferred cost, only if parametric export is pursued: replay determinism
(seedable RNG, pinned iteration order, avian's same-platform caveat) and
op-level migration. Because RON stays canonical, none of this is on the critical
path — it is opt-in.

## Build vs. reuse (ecosystem survey, 2026-07-08)

- **steel** — actively maintained embeddable Scheme, battle-tested as Helix's
  plugin language, with a package manager (forge). Chosen: it is the
  *authoring-time* engine, so its VM weight never reaches the runtime loop, and
  its ecosystem is the extensibility multiplier (shareable `.scm` gadget
  libraries over our builtins-as-ABI). Trade-off: larger dependency/compile
  surface — acceptable because it is cold-path.
- **ketos** — lighter, Rust-native, built-in step-limit. Fallback if steel's
  build weight bites; quieter maintenance rubs against exact-pinning.
- **`bevy_mod_scripting`** — proves the reflection-bridge approach at scale, but
  makes scripts *direct* world mutators — the exact governance we reject. Study
  the bridge mechanics; do not adopt the poke-the-world model.
- **Constraint / CAD kernel** — named escalation path for the deferred
  declarative solver (Rust 2D-constraint options are thin; a CAD-kernel crate or
  Wolfram/`argmin` sit *behind* the declarative interface, never in core).
- **Wolfram** — feature-gated backend behind the symbolic-op builtins
  (`solve`/`grad`/`simplify`), never a default dependency. Free engine is a
  *developer* license — fine as a locally-configured backend, not for
  redistribution. Wolfram maintains Rust bridges (`wstp`); `wolframscript` is
  the low-effort first cut. Same "escalation path behind a stable interface"
  discipline as `fidget` in the SDF decision.

`ShapeDef` stays an **opaque foreign value** in scripts (built via constructor
builtins, passed as handles). Given the representation is still in flux, this is
insulation, not loss: the constructor API is a stable surface while the enum
churns. Script-side tree-walking is additive later via accessor builtins.

## Module layout (keeps boundaries honest)

```
src/script/
  mod.rs      # ScriptPlugin, ScriptError (thiserror), embed + panic guard + fuel budget
  values.rs   # foreign types (ShapeDef/Vec2/StableId) + geometry constructors — NO ECS, proptestable
  bridge.rs   # the only ECS-touching part: exclusive run_script system beside dispatch;
              #   operation registry; scene verbs EMIT INTENTS; reads via physics::queries
  driver.rs   # Driver component + named-signal dataflow + Tier-B kernel eval seam
  kernel.rs   # DSL-subset → flat numeric kernel (the hot-path lowering)
src/ui/console.rs   # REPL panel: input-queue + output-log resources only (no World access)
```

Allowed imports for `src/script/`: `domain`, `command::intent`,
`physics::queries`. **Not** `avian2d` (reads via facade, writes via intents →
invariant #3) and **not** `bevy_egui` (console's job → invariant #4). Add these
rules to `tests/boundaries.rs`.

## Cross-cutting decisions (resolved defaults)

- **Driver timing** — Tier-B kernels run in the physics schedule
  (`FixedUpdate`/substep), never render-frame `Update`, for frame-rate
  independence and reproducibility.
- **API/migration** — versioned operation registry; builtin renames via
  aliases, mirroring RON scene migration.
- **Live-driver observability** — a driver producing NaN freezes-and-highlights
  per-driver rather than silently poisoning the sim.
- **Safety** — fuel/step budget kills runaway authoring scripts; eval wrapped in
  `catch_unwind` + converted to `ScriptError`; no file/net from scripts by
  default. Scripts cannot break invariants (they only emit intents).

## Linchpin spikes (do before committing code)

1. **`bevy_reflect` ↔ steel value bridge.** The whole low-boilerplate
   "everything programmable" vision rests on writing this converter *once* so
   every `#[derive(Reflect)]` type becomes scriptable. Narrow spike: reflect one
   intent + one settings resource, round-trip through steel, dispatch through
   the seam. If clean → cheap endgame; if painful → rethink (hand-rolled core
   returns to the table).
2. **DSL-subset → Tier-B kernel over a query.** Prove a compiled numeric
   expression drives N components per fixed-step at target scale with no VM in
   the loop and no per-element allocation. This de-risks the particle/fluid
   ceiling.

## Milestone impact (accrete, don't big-bang)

Programmability is not a scheduled milestone; it is a seam each milestone is
built *through*:

- **M16 (tools rework)** → tools adopt a `ToolContext` facade →
  `(preview, commit-intent)` shape, so a scripted tool is later just a closure
  implementing the same interface.
- **M17 (grid basis, axis-lock)** → grid basis is a settings-*operation* in the
  registry; UI and script drive the same op.
- **M18 (gravity widget, sim-settings UI)** → the widget writes through the same
  settings-operation the script calls.
- **Ongoing** → derive `Reflect` on intents, settings, and domain components as
  they are touched; this is the substrate everything stands on.

Feature-level phasing once the spikes pass:

- **P0** — embed steel; `ShapeDef` foreign type + geometry constructors;
  `ScriptError` + guards. Pure, proptestable.
- **P1** — scripted editing: exclusive `run_script` emitting intents; REPL
  panel; `--script foo.lisp` headless (doubles as test fixtures); one script run
  = one undo entry.
- **P2** — drivers as named-signal dataflow + Tier-B kernel seam; `defparam` →
  auto-slider; live probes / tracers / plots (data-out seam).
- **P3** — targeted symbolic ops: forward-mode autodiff `(grad …)`, 1D root-find
  `(solve …)`, symbolic **field forces** (unifies symbolic math with the SDF
  substrate; the flagship demo). Optional parametric `.scm` scene export if the
  parametric workflow proves wanted (RON stays canonical regardless).

## Open questions (deferred, named so they are not forgotten)

- Full **sim-event** design (spawn/destroy during play, triggers) — non-undoable
  category, needs its own pass well beyond the first spike.
- Whether parametric `.scm` scene export is ever pursued — and if so, replay
  determinism across avian upgrades and the op-migration format. Opt-in; RON
  canonical means this is never forced.
- Whether particle/fluid runs on avian, a bespoke CPU solver, or GPU compute —
  the derived-bulk-population boundary is chosen now precisely to keep this open.
- Constraint-solver interface shape (declarative relations vs. objective
  functions) — designed only when a concrete need appears.
- Unit-typed value layer — the tag representation, added when the raw-number
  floor starts causing errors.
