# Brainstorm: developer experience & packaging (hot reload, workspace split, feature crates)

Status: **brainstorm — not ratified.** (2026-07-11)

Prompted by two questions: (a) how do we make the edit→see loop fast
(hot-reloading and friends), and (b) the project previously decided *against* a
multi-crate workspace (boundaries enforced by `tests/boundaries.rs` instead) —
does that still hold now that the codebase has grown and large feature families
are queued (particles/fluids/MPM; node dataflow, advanced plotting, SDF color
mixing; raytracing, transparency, shadows, 2.5D extensions, gizmos, SVG export)?

---

## 1. The iteration loop today (facts)

- **One crate**, ~15.8 k LOC across 93 files in `src/`, 10 modules. 725 locked
  dependencies (Bevy 0.19 + avian 0.7 + egui 0.41 + steel, exact-pinned).
- `profile.dev`: our code at `opt-level = 1`, deps at 3, `debug =
  "line-tables-only"` — the last one added because **linking several
  Bevy-sized test binaries exceeded container memory**. That comment is a
  symptom worth reading twice (§2.3).
- `.cargo/config.toml` sets `linker = clang`; the `lld` line is **commented
  out**. So every rebuild pays a stock-linker link of a Bevy-sized binary.
- **Twelve integration-test files** under `tests/`, each a separate binary
  linking the full Bevy stack.
- No `dev` cargo feature: no `bevy/dynamic_linking`, no `bevy/file_watcher`.
- The layers split cleanly on import evidence: `geometry/` touches Bevy only
  via `bevy::math` (re-exported glam, already a direct dependency);
  `script/kernel.rs` is pure; `core/` is nearly pure. `domain/` is **not**
  engine-free anymore — since the de-adapter collapse
  (`docs/physics-deadapter-decision.md`) authored physics state *is* avian
  components (`domain/props.rs` imports `avian2d`). That is by design, not rot.

Conclusion up front: most of the day-to-day pain is *link time and test-binary
multiplication*, not crate structure. Fix those first; they are independent of
any packaging decision and cheap enough to do this week.

## 2. Cheap wins first (no architecture change)

### 2.1 Fast linker

Uncomment/enable `lld` (or install `mold` and use that) in
`.cargo/config.toml`. On a Bevy project this is routinely the single largest
iteration-loop win — link time dominates incremental rebuilds. If it was
commented out because some environment lacked `lld`, gate it per-machine
(`.cargo/config.toml` is not the only place; a documented one-liner in the
README dev section is fine) rather than losing it everywhere.

### 2.2 A `dev` cargo feature

```toml
[features]
dev = ["bevy/dynamic_linking", "bevy/file_watcher"]
```

- `bevy/dynamic_linking` links Bevy as a dylib in dev builds — large link-time
  reduction, zero code change. Never ships (release builds don't pass the
  flag); CI keeps building without it so the static path stays green.
- `bevy/file_watcher` gives asset hot-reload, which becomes load-bearing the
  moment scripts are assets (§3.1).

`cargo run --features dev` becomes the documented inner loop.

### 2.3 Collapse the integration-test binaries

Twelve test binaries × full Bevy link is where both CI time and the container
memory ceiling come from (the `line-tables-only` workaround was treating the
symptom). The standard fix: one umbrella binary —

```
tests/it/main.rs        // declares `mod csg; mod joints; ...`
tests/it/csg.rs         // current tests/csg.rs, unchanged content
...
```

One link instead of twelve, tests unchanged, `cargo test --test it csg::`
still filters. Keep `tests/boundaries.rs` as its own tiny binary (it links no
Bevy and CI runs it as a separate early step). `cargo-nextest` is a further
optional win (per-test process isolation, better parallelism) but the binary
collapse is the big one.

### 2.4 Background check loop

`bacon` (or `cargo watch -x "clippy --all-targets"`) so type/lint errors
surface at save-time, not run-time. Trivial, but it changes how often you
actually pay a link.

## 3. Hot reloading — three tiers, ordered by architectural alignment

The key observation: **this project already has a hot-reload architecture; it's
the scripting layer.** The north star says every tool, menu action, and sim
feature becomes scriptable. Every feature that lands as a `.scm`-registered
verb/tool/action is *already* reloadable without touching rustc. So the tiers
below are ordered by how much they lean into what the project is already
becoming, rather than fighting Rust's compile model.

### 3.1 Tier 1 — scripts as hot assets (do this)

Today `--script foo.scm` runs once at startup and the REPL console re-runs by
hand. The increment:

- Watch a directory (e.g. `assets/scripts/`) via `bevy/file_watcher` (or a
  small `notify` watcher in `script/bridge.rs` if we don't want scripts in the
  asset server), and re-run a file when it changes.
- **Idempotent re-registration is the design work**: `register-action` /
  future `register-tool` / `defsignal` must *replace by name*, not append, so
  re-running a file converges instead of duplicating. This is a good
  constraint to adopt now while the registry surface is small — it also makes
  the REPL's re-run story coherent.
- Governance is already solved: a re-run script goes through the same
  Edit/Config/Query/EditorState seams; there is no new mutation path. Edits it
  performed earlier are commands on the undo stack and *should not* be
  replayed — a reloaded script re-registers definitions; it does not re-apply
  history. Splitting "definitions" (idempotent) from "actions" (one-shot,
  REPL/CLI-triggered) in the user guide (`docs/scripting.md`) captures that.

Payoff compounds forever: every feature migrated to the extension surface
(P3: lisp-authored tools, context actions, field forces) inherits sub-second
reload for free. This is also the cheapest tier — no new deps, no unstable
tooling.

### 3.2 Tier 2 — state-preserving restart (nearly free, do this too)

"Loading a scene has no special cases" (spawn authored records, sync systems
rebuild everything derived) means **restart-with-state is almost a one-liner**:

- On exit (and/or every N seconds in dev), autosave the scene +
  settings resources to `target/dev-session.ron`.
- `--resume` (or `dev` feature default) loads it at startup, restoring scene,
  camera, and editor settings.

Combined with §2.1/§2.2, the loop becomes: save file → ~few-seconds rebuild →
app reopens *in the same scene, same camera, same tool*. For a single
maintainer this captures most of the value of true code hot-reload at a tiny
fraction of the complexity, and it doubles as crash recovery.

Worth stating: this only works because of invariant #5. It is a nice concrete
dividend of the authored/derived split and a reason to keep classifying new
state correctly (per-particle state must be derived or it poisons autosave).

### 3.3 Tier 3 — Rust code hotpatching (timeboxed spike, don't commit to it)

Bevy ships an **experimental `hotpatching` feature** (subsecond-based, driven
by the Dioxus `dx` CLI) that live-patches system bodies; `dexterous_developer`
is the third-party alternative. Honest assessment for this repo:

- Best case: tweak-a-gizmo / tune-a-force loops without relaunch.
- Caveats: experimental; system-body changes only (no struct layout/ECS schema
  changes); another toolchain moving part in an exact-pinned repo; and our
  Tier 1+2 already cover the authoring-loop cases where hot iteration matters
  most. The per-frame numeric hot path is the *kernel*, which is script-side —
  Tier 1 reloads it.
- Recommendation: a **timeboxed spike behind a `hotpatch` feature**, evaluated
  against `docs/bevy19-notes.md`-style verification (0.19 API reality, not
  training-data memory). Adopt only if the spike shows it survives our egui +
  avian + steel stack. It is an accelerator, not a foundation — nothing else
  in this document depends on it.

## 4. Reassessing the single-crate decision

### 4.1 What the current mechanism gets right

`tests/boundaries.rs` (text-scan: egui→`ui/`, steel→`script/`,
`CommandStack`→`command/`, `Serialize`→authored/persist) has three virtues a
crate split cannot match:

1. **Cheap to change.** The de-adapter collapse retired the avian rule by
   deleting one test. Had physics been a crate with an engine-agnostic API,
   that collapse would have been a workspace surgery. The project's own recent
   history is a warning: **a crate boundary is the most expensive form of a
   boundary, and we just finished paying down one boundary that didn't earn
   its keep.**
2. It can express rules crates can't (a *type* confined to a module; serde
   derives confined to authored data) — these stay as tests regardless.
3. Zero cost on the accretion style: refactors sweep across `src/` freely.

### 4.2 What has changed

Three things genuinely shifted since the decision:

- **Compile/link scope.** At ~16 k LOC the crate is fine, but each queued
  feature family (MPM, raytracer, node editor) is plausibly 5–15 k LOC *plus
  heavy new deps* (wgpu compute pipelines, a node-graph widget). In a single
  crate, everyone pays for everything on every build, always.
- **Test islands.** `geometry/` is the property-test-heavy pure math layer,
  and it currently only runs inside binaries that link Bevy. As a crate on
  plain `glam`, its proptest loop is sub-second and its purity is
  compiler-enforced instead of convention.
- **Optionality.** Feature families the user may not want in every build
  (GPU raytracer, MPM solver) want cargo features — and features compose much
  more cleanly along crate lines than inside one crate's module tree.

### 4.3 Recommendation: a hybrid, split along the *pure/ECS* line, not the module map

Do **not** mirror the 10 `src/` modules into 10 crates — the ECS layers
(command/physics/interaction/render/ui/persist/script-bridge) are exactly the
code the accretion recipes sweep across ("a new joint touches physics + render
+ ui + command"), and crate-izing them taxes every recipe in §6 of
`agent-context.md`. Instead:

```
crates/
  gradiance-geometry   # SDF eval, polygonize, extrusion, snapping math.
                       # deps: glam, lyon, clipper2, serde. NO bevy.
  gradiance-kernel     # the Tier-B numeric tape. deps: ~none. NO bevy, NO steel.
  gradiance            # everything else (the app), depends on the above.
```

(`core/` can fold into `gradiance-geometry` or stand alone; it's 263 lines —
whichever reads better. `domain/` **stays in the app**: post-collapse it is
avian-shaped by design and extracting it buys nothing.)

- This is mechanical: geometry already imports only `bevy::math`; swap to
  `glam::` and it compiles Bevy-free. The kernel is already pure.
- Purity ("`src/geometry/` has no ECS imports", "the kernel never allocates or
  touches the VM") stops being convention and becomes `cargo tree -p
  gradiance-geometry` truth. The perf rule's *structure* (kernel can't call
  steel) becomes unbuildable rather than reviewable.
- The boundary tests **stay** for the intra-app rules (egui, steel,
  CommandStack, Serialize) — workspace and text-scan enforcement compose; one
  new test denies `bevy::` inside `crates/gradiance-geometry`… except that's
  now the compiler's job. Extend `tests/boundaries.rs` only where crates can't
  reach.
- Adopt `[workspace.dependencies]` so the exact-pins stay defined once.

### 4.4 Triggers for further splits (write them down, don't pre-split)

Split a feature into its own crate **when the first of these fires**, not
before:

1. It brings a **heavy dependency** the base editor shouldn't pay for
   (wgpu-compute kernels, a node-graph widget crate, an SVG writer is *not*
   heavy).
2. It should be **feature-flagged out** of some builds/targets.
3. Its **inner loop is hurt** by linking the app (pure solvers with their own
   test/bench suites — MPM math is the archetype).
4. Two agents/sessions need to work it **in parallel** without rebasing over
   each other's `src/` churn.

Until a trigger fires, a new feature is a module + a boundary-test line, same
as today.

## 5. Mapping the queued feature families onto packages

The unifying idea first: **the operation registry + intents + the named-signal
store are already the internal API between packages.** A feature package, when
one exists, is: *a Bevy plugin that registers ops (writes), signals (reads),
and kernels (per-frame), plus optionally a UI panel.* Nothing reaches into
another package's modules; everything composes at the same three seams scripts
use. This means the packaging question and the scripting north star are the
same investment — the registry is the plugin ABI.

| Feature | Authored vs derived | Seam | Home | Split trigger (§4.4) |
|---|---|---|---|---|
| Particles / fluids / MPM | Emitters/parameters authored; per-particle state **derived** (never saved, never undone — the perf rule) | kernels (Tier B) + Config; emitter spawn via Edit | `gradiance-sim-mpm` crate when it lands | #1/#3 immediately if GPU (wgpu compute); #3 regardless — a pure solver core with its own bench suite, coupled one-way (bodies → field → particles) at first |
| Node dataflow programming | Graph is authored (it's a program) | It **is** the P2 sensor/modulator/actuator layer: nodes = registry ops + signals + kernels; the editor is a *projection of the registry* | `gradiance-ui-nodes` crate | #1 the day it adopts a node-widget dep (`egui_snarl` etc.). The egui-confinement rule generalizes: "egui only in ui crates" |
| Advanced live plotting | Plot configs authored-ish (EditorState), data derived | reads named-signal store via the facade | stays in `src/ui/` | #1 only if it adopts `egui_plot`/similar |
| SDF color mixing | Authored (`Appearance` grows a field-blend) | Edit | `geometry` (field eval) + `render` (material) — core, not a package | — (it's substrate, like CSG) |
| Analytic raytracing | Derived entirely (a renderer) | reads `ShapeDef` through geometry | `gradiance-render-rt` crate | #1+#2 immediately: shader-heavy, optional per build; the SDF tree makes it *analytic* — a flagship consumer of `gradiance-geometry` as a standalone crate |
| Transparency, shadows, 2.5D extensions, better gizmos | Derived | render sync | stay in `src/render/` | — (they modify the main pipeline; splitting churns) |
| SVG export | Pure consumer of `polygonize` contours + `Appearance` | Query (read-only) | ideal **pilot extraction** | do it *as* the workspace pilot: tiny, pure, proves the geometry-crate cut and the workspace mechanics end-to-end |

Two classification landmines to call out for the sim family, because they are
the "most common architectural mistake" §9 already warns about:

- Per-particle state must be **derived** (rebuilt from authored
  emitters + elapsed sim), or autosave (§3.2), undo, and RON all bloat and
  break. Determinism-on-reload comes from seeding, not persistence.
- MPM/fluids may not use avian at all — post-collapse that's fine (no adapter
  to honor), but define the coupling contract explicitly (which fields flow
  bodies→particles and particles→bodies, and in which schedule slot) *before*
  the crate exists, or it will grow ad-hoc reads into avian internals.

## 6. Agent DX: the contract docs are part of the toolchain

Found while researching, and worth elevating: **`CLAUDE.md` and
`docs/agent-context.md` still state invariant #3 (avian confined to
`src/physics/`, engine swappability) — which was retired on 2026-07-09** by
`docs/physics-deadapter-decision.md`, whose own step 4 ("rewrite #3/#5 in
CLAUDE.md, refresh architecture.md") appears unlanded. `tests/boundaries.rs`
already reflects the new world; the contracts don't.

In an agent-driven repo the contract docs *are* developer experience — an
agent reading CLAUDE.md today will architect new physics work around a seam
that no longer exists. Fixing this is arguably the highest-leverage DX item in
this document and should land before any of the rest. (A cheap guard worth
considering: `tests/boundaries.rs` grows a test asserting CLAUDE.md doesn't
mention retired rules — the file is already doing text-scan enforcement, and
doc drift has now happened once.)

## 7. Suggested sequence (smallest first, each independently valuable)

1. **Fix the stale contracts** (deadapter step 4: CLAUDE.md, agent-context.md,
   architecture.md). One session, pure docs.
2. **Linker + `dev` feature** (§2.1–2.2), measure rebuild time before/after
   and record it in the README dev section.
3. **Integration-test binary collapse** (§2.3) — also likely lets
   `profile.dev` debuginfo be revisited.
4. **Tier 1 + Tier 2 hot reload** (§3.1–3.2): script file-watch with
   replace-by-name registration semantics; dev autosave/`--resume`.
5. **Workspace pilot** (§4.3): extract `gradiance-geometry` + `gradiance-kernel`
   (+ optionally prove it with the SVG exporter as first external consumer).
6. **Hotpatching spike** (§3.3), timeboxed, feature-gated, adopt only on a
   clean result.
7. **First real feature crate** when its §4.4 trigger fires — on current
   roadmap ordering that is most plausibly the raytracer or MPM.
