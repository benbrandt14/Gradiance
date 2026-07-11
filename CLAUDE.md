# Gradiance — Agent Contract

Algodoo-inspired 2.5D physics sandbox. Bevy 0.19 + Avian2d 0.7 + bevy_egui 0.41 (exact-pinned).
**Your API knowledge is probably stale for these versions — read `docs/bevy19-notes.md` before writing Bevy/avian/egui code.**

## Invariants (CI-enforced; violating these fails the build)

1. **All world mutation goes through commands.** Tools and UI emit intent events
   (`command::intent`); only `command::dispatch` drains them, builds `GameCommand`s, and
   touches `CommandStack`. Commands mutate authored components only.
2. **Tools/UI never mutate directly.** During a drag gesture, tools may write transient
   preview state (kinematic-held `Transform`, gizmos) — never authored components, never
   the command stack. One gesture commits exactly one command on release.
3. **Identity is `StableId`; raw `Entity` is never persisted or cross-referenced.**
   (The former avian-confinement rule was retired by the de-adapter collapse —
   `docs/physics-deadapter-decision.md`. avian is used directly wherever physics is
   done; authored physics state *is* avian components. `physics::queries` remains as a
   convenience/DRY read cut-point, not an abstraction boundary.)
4. **`egui`/`bevy_egui` is imported only inside `src/ui/`.** UI reads component copies
   and emits intents; it never mutates authored components directly. Sole exception:
   editor **settings resources** (`GridSettings`, `SnapConfig`, `SimSettings`) are
   non-authored configuration and may be written by UI; seams consume them via change
   detection (physics applies `SimSettings` — UI edits authored state only via intents).
5. **Authored vs derived:** components in `src/domain/` (+ `StableId`) plus the authored
   avian components (`RigidBody`, `Friction`, `Restitution`, `ColliderDensity`,
   `GravityScale`, `Sensor`, `LockedAxes`) are the save file. Derived state (`Collider`,
   `Mass`, contacts, meshes, materials, live avian joint entities) is rebuilt by
   `Changed<>`-driven sync systems and is never serialized, never captured in undo
   records, and never read by commands.

## Conventions

- Identity: `core::ids::StableId` (UUID) on every authored entity; never persist or
  cross-reference raw `Entity`. Joints reference bodies by `StableId`.
- Units: `PIXELS_PER_METER = 100.0`; gravity `(0, -1000)`; polygon vertices are
  centroid-relative at authoring time (CSG reshapes may leave the origin
  off-centroid); extrusion depth = collision layer bits × `LAYER_HEIGHT = 10.0`
  (bit 0 front … bit 31 back).
- Geometry: `ShapeDef` is an SDF tree (analytic leaves + `Csg`/`Placed` nodes;
  see `docs/sdf-geometry-decision.md`). `geometry::polygonize` is the single
  discretization point — every derived consumer (colliders, meshes, snapping)
  reads contours through it; never contour a field anywhere else.
- `src/geometry/` is pure (no ECS imports) — put all testable math there.
- Errors: `thiserror` enums; no `unwrap`/`expect`/`panic!` outside tests (clippy denies).

## Scripting & symbolic modeling (foundation landed — read `docs/script-lisp-decision.md`; user/verb guide in `docs/scripting.md`)

The accepted plan makes the whole tool programmable via a Lisp/DSL. It is being
built by *accretion*, not big-bang: honor these when your work touches an
adjacent seam, so the layer lands uniformly instead of as an unmaintained
side-car. The P1 substrate is in place — the operation registry
(`src/script/registry.rs`), edit/config/query/editor verbs + REPL console +
`--script` loader (`src/script/bridge.rs`, `src/ui/console.rs`); adding a verb
follows the recipe in `docs/scripting.md`.

- **No new mutation path.** Scripting reuses the *same* intent seam as tools/UI
  (invariants 1–2). A future operation registry may only dispatch through
  intents / settings resources — never `get_mut` an authored component.
  Governance is asymmetric: **reads are total** (script/plotters may query any
  component or resource), **writes are seam-mediated**. Keep the
  `physics::queries` read facade complete — live plotters and scripts read
  through it.
- **Two tiers, one language — the perf rule.** The scripting VM (authoring/cold
  path) must **never** run in the per-frame loop. Continuous drivers lower to a
  compiled, allocation-free numeric kernel (`src/script/kernel.rs`, Tier B) that
  runs over queries/buffers in one system. Bulk/particle updates are *derived*
  (never commands, never undo-recorded, never persisted).
- **`src/script/` is pure at the core** (`kernel`, `reflect_bridge`), like
  `src/geometry/` — put testable math there. `src/script/bridge.rs` is the one
  ECS-touching seam (exclusive `run_scripts` → intents). `steel` is a
  first-class dependency but may be imported **only** in `src/script/`
  (`bridge`/`reflect_bridge`), enforced by `tests/boundaries.rs`.
- **Persistence stays single-format RON** of materialized authored state; the
  operation registry is a runtime construct, not a second on-disk representation.
- **When you rework tools/settings:** shape tools as `ToolContext →
  (preview, commit-intent)`, keep config edits on the settings resources, and
  derive `bevy_reflect::Reflect` on intents/settings/domain *as you touch them*
  (the registry binds to reflection). Do not blanket-derive ahead of need.

## Build & test

Toolchain: **rustc ≥ 1.95.0** (Bevy 0.19's floor; CI uses `@stable`). Native
builds also need `libasound2-dev libudev-dev`.


```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings   # lint gate == CI
cargo test                                   # includes tests/boundaries.rs (layer rules)
cargo run                                    # native app; needs libasound2-dev libudev-dev
cargo run --features dev                     # dev inner loop: dynamic linking + asset hot-reload
```

Integration tests live in the single umbrella binary `tests/it/` (one Bevy-sized
link instead of one per file) — add new integration tests as a module there
(`tests/it/main.rs` declares them). `tests/boundaries.rs` stays its own tiny binary.
The `dev` feature is dev-only (never release/CI). Linking uses `lld` via
`.cargo/config.toml` — needs `clang`+`lld` installed.

Every change must leave fmt+clippy+test green; milestones are not done otherwise.
