# Gradiance — Agent Contract

Algodoo-inspired 2.5D physics sandbox. Bevy 0.19 + Avian2d 0.7 + bevy_egui 0.41 (exact-pinned).
**Your API knowledge is probably stale for these versions — read `docs/bevy19-notes.md` before writing Bevy/avian/egui code.**

## Workspace layout (the layer diagram is the dependency graph)

One package per architectural layer under `crates/`, plus the root package
`gradiance` (app shell: plugin group, prelude, `main`, the integration
tests). The root re-exports each package under its layer name
(`gradiance::command`, `gradiance::geometry`, …), so test/doc paths read as
before the split. Dependency edges point strictly downward — a boundary
violation is a **compile error**, not a review comment:

```text
kernel → (nothing)                  units → (nothing, +bevy for Reflect)
core → bevy                         geometry → core
domain → core, geometry             scene → core, domain
optimize → core, geometry (+bevy for Reflect)
physics → core, domain, geometry, units
signal → kernel, domain, physics    command → scene, signal (+ lower)
persist → scene, command            interaction → command, optimize, persist (+ lower)
render → interaction, units (+ lower)   script → command, signal (+ lower)
ui → everything except render
```

`gradiance-units` is the typed-SI-quantity crate (`docs/units-decision.md`):
a bottom node with no `gradiance-*` deps but a minimal `bevy` surface (its
quantity newtypes derive `Reflect`, like the geometry shape tree). It owns
the single px↔SI seam `units::world` (`PIXELS_PER_METER`).

`gradiance-optimize` is the layout-solver crate (`docs/optimize-decision.md`):
a pure geometric search over poses — convex hulls, SAT, a weighted objective,
and a `Solver` trait with three interchangeable strategies (one of them
`argmin`-backed L-BFGS over an analytic gradient, one a deliberately naive
baseline the others are CI-asserted to beat). Like `geometry` it has
no systems and no queries; its only bevy surface is `Resource`/`Reflect` on
the `PackConfig` rulebook. **It never touches the physics engine** — packing
is an optimization with an objective, not a simulation (see the crate docs
for why that distinction is load-bearing). The ECS half — gathering a
selection, stepping the run across frames, drawing the ghost, committing one
transform command — is `interaction::pack`.

Rationale, coupling data, and the roadmap→package feature tree:
`docs/workspace-plan.md`.

## Invariants (CI-enforced; violating these fails the build)

1. **All world mutation goes through commands.** Tools and UI emit intent events
   (`command::intent`); only `command::dispatch` drains them, builds `GameCommand`s, and
   touches `CommandStack`. Commands mutate authored components only. Adding a
   command = one row in the `command_intents!` table in `dispatch` (the row
   registers the message + reflected type and dispatches it) plus the
   intent/command types.
2. **Tools/UI never mutate directly.** During a drag gesture, tools may write transient
   preview state (kinematic-held `Transform`, gizmos) — never authored components, never
   the command stack. One gesture commits exactly one command on release.
3. **Identity is `StableId`; raw `Entity` is never persisted or cross-referenced.**
   (The former avian-confinement rule was retired by the de-adapter collapse —
   `docs/physics-deadapter-decision.md`. avian is used directly wherever physics is
   done; authored physics state *is* avian components. `physics::queries` remains as a
   convenience/DRY read cut-point, not an abstraction boundary.)
4. **`egui`/`bevy_egui` is a dependency of `crates/gradiance-ui` only** (and `steel`
   of `crates/gradiance-script` only) — enforced by the package graph and re-checked
   by `tests/boundaries.rs`. UI reads component copies and emits intents; it never
   mutates authored components directly. Sole exception: editor **settings
   resources** may be written by UI directly; seams consume them via change
   detection (physics applies `SimSettings` — UI edits authored state only via
   intents). Those resources split by *what they describe*, not where they are
   edited:
   - **scene content** (`SimSettings`, `RenderSettings`, `LightingSettings`,
     `ScenerySettings`, and the signal-graph resources) — part of the document.
     Saved **and** undoable: `command::commit_settings_edits` watches them by
     value and records a settled edit as one undo step (a whole slider drag
     collapses into one). Bevy change flags are *not* usable here — the settings
     window calls `set_changed()` unconditionally and writes through
     `bypass_change_detection`.
   - **workstation config** (`GridSettings`, `SnapConfig`) — authoring aids that
     belong to the person, not the document. Saved with the scene, never in
     undo history, so reverting an edit can't move someone's grid.

   The split lives in one place: `scene::records::EnvironmentRecord::scene_content_eq`
   / `apply_scene_content`. A new settings resource must be classified there.
5. **Authored vs derived:** components in `gradiance-domain` (+ `StableId`) plus the
   authored avian components (`RigidBody`, `Friction`, `Restitution`, `ColliderDensity`,
   `GravityScale`, `Sensor`, `LockedAxes`) are the save content. The save **format**
   (records, RON, version migrations) is `gradiance-scene` — records are the shared
   unit of undo capture and persistence. Derived state (`Collider`,
   `Mass`, contacts, meshes, materials, live avian joint entities) is rebuilt by
   `Changed<>`-driven sync systems and is never serialized, never captured in undo
   records, and never read by commands.

## Conventions

- Identity: `core::ids::StableId` (UUID) on every authored entity; never persist or
  cross-reference raw `Entity`. Joints reference bodies by `StableId`.
- Units: `PIXELS_PER_METER = 100.0`; gravity `(0, -1000)`; polygon vertices are
  centroid-relative at authoring time (CSG reshapes may leave the origin
  off-centroid); depth is a continuous authored `DepthBand {near, far}`
  (world units into the screen, body spans z ∈ [−far, −near]); collision
  layer bits (`LAYER_HEIGHT = 10.0` slabs, bit 0 front) are *derived* from
  the band — collision layer ≡ visual depth.
- Array repeats: `geometry::array` computes the **flush pitch** — the smallest
  translation along a direction that clears a selection from itself (exact for
  convex pieces, via the same SAT axes as overlap). `interaction::tools::array_tool`
  turns a `Ctrl`-drag on a scale handle into an `ArrayMode`; `command::array_cmd`
  expands any mode into `CopyPlacement`s — **pure translations** — so adding a
  pattern is one match arm and no new cloning logic. Per-copy tweens were tried
  and removed; see `docs/array-decision.md` before re-adding them.
- Geometry: `ShapeDef` is an SDF tree (analytic leaves + `Csg`/`Placed` nodes;
  see `docs/sdf-geometry-decision.md`), owned by `gradiance-geometry` (re-exported
  as `domain::shape`). `geometry::polygonize` is the single
  discretization point — every derived consumer (colliders, meshes, snapping)
  reads contours through it; never contour a field anywhere else.
- `gradiance-geometry` is the math layer: no systems, no resources, no queries —
  put all testable math there (its only bevy surface is the
  `Component`/`Reflect` derives on the shape tree). `gradiance-kernel` is fully
  pure (no bevy at all).
- Errors: `thiserror` enums; no `unwrap`/`expect`/`panic!` outside tests (clippy denies).

## Scripting & symbolic modeling (foundation landed — read `docs/script-lisp-decision.md`; user/verb guide in `docs/scripting.md`)

The accepted plan makes the whole tool programmable via a Lisp/DSL. It is being
built by *accretion*, not big-bang: honor these when your work touches an
adjacent seam, so the layer lands uniformly instead of as an unmaintained
side-car. The P1 substrate is in place — the operation registry
(`crates/gradiance-script/src/registry.rs`), edit/config/query/editor verbs +
REPL console + `--script` loader (`crates/gradiance-script/src/bridge.rs`,
`crates/gradiance-ui/src/console.rs`); adding a verb follows the recipe in
`docs/scripting.md`.

- **No new mutation path.** Scripting reuses the *same* intent seam as tools/UI
  (invariants 1–2). A future operation registry may only dispatch through
  intents / settings resources — never `get_mut` an authored component.
  Governance is asymmetric: **reads are total** (script/plotters may query any
  component or resource), **writes are seam-mediated**. Keep the
  `physics::queries` read facade complete — live plotters and scripts read
  through it.
- **Two tiers, one language — the perf rule.** The scripting VM (authoring/cold
  path) must **never** run in the per-frame loop. Continuous drivers lower to a
  compiled, allocation-free numeric kernel (`gradiance-kernel`, Tier B) that
  runs over queries/buffers in one system. The package graph states this:
  `signal` depends on `kernel`, not on `script`. Bulk/particle updates are
  *derived* (never commands, never undo-recorded, never persisted).
- **`steel` is confined to `gradiance-script`** (its manifest is the only one
  declaring the dependency; `tests/boundaries.rs` re-checks the source).
  `bridge.rs` is the one ECS-touching seam (exclusive `run_scripts` → intents);
  `reflect_bridge` stays pure glue.
- **Persistence stays single-format RON** of materialized authored state
  (`gradiance-scene`); the operation registry is a runtime construct, not a
  second on-disk representation.
- **When you rework tools/settings:** shape tools as `ToolContext →
  (preview, commit-intent)`, keep config edits on the settings resources, and
  derive `bevy_reflect::Reflect` on intents/settings/domain *as you touch them*
  (the registry binds to reflection). Do not blanket-derive ahead of need.

## Build & test

Toolchain: **rustc ≥ 1.95.0** (Bevy 0.19's floor; CI uses `@stable`). Native
builds also need `libasound2-dev libudev-dev`.


```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # lint gate == CI
cargo test --workspace                       # includes tests/boundaries.rs (layer rules)
cargo run                                    # native app; needs libasound2-dev libudev-dev
cargo run --features dev                     # dev inner loop: dynamic linking + asset hot-reload
```

Shared lint policy lives in `[workspace.lints]` (root `Cargo.toml`); every
member inherits it (`[lints] workspace = true`). Shared dependency versions
live in `[workspace.dependencies]` — the exact-pin line is asserted by
`tests/boundaries.rs`, which also asserts the **crate DAG itself** (the
layer diagram above, as data): adding a `gradiance-*` dependency edge to a
member manifest requires a matching row in that test — a deliberate,
reviewed architecture change, never a side effect.

**CI owns the heavy checks — do not run these in an agent container:**
coverage (instrumented builds; the `coverage` CI job runs `cargo llvm-cov`
over the `coverage` profile with a 50%-lines floor) and the module-level
`cargo coupling` report (informational CI job). CI builds run `--locked`:
never regenerate `Cargo.lock` as a side effect; a lockfile diff is a
reviewed change.

Integration tests live in the single umbrella binary `tests/it/` of the root
package (one Bevy-sized link instead of one per file) — add new integration
tests as a module there (`tests/it/main.rs` declares them); they drive the
whole app through `gradiance::…` re-exports. `tests/boundaries.rs` stays its
own tiny binary. Pure-math unit tests belong in their layer crate
(`cargo test -p gradiance-kernel` runs without linking the app).
The `dev` feature is dev-only (never release/CI). Linking uses `lld` via
`.cargo/config.toml` — needs `clang`+`lld` installed.

Every change must leave fmt+clippy+test green; milestones are not done otherwise.
