# Workspace refactor plan — from monolith to packages

Status: **executed** (this document is the plan the split followed; kept as
the rationale record). Coupling data from `cargo coupling --ai
--exclude-tests` (cargo-coupling 0.3.7), 2026-07-20, plus a manual
`crate::<module>` import census.

## Why now

The crate hit 27 k lines across 11 top-level modules. The architecture
contract (`CLAUDE.md`, `docs/architecture.md`) was healthy but enforced
only by a text-scanning test (`tests/boundaries.rs`) — and the census
showed the first cracks forming exactly where a text scan can't see them:

- **Grade C** (score 0.86): 13 high / 44 medium coupling issues.
- **Cycles at module granularity** that a crate split would have made
  unbuildable:
  - `command ↔ persist` — the save-format types (`command::snapshot`)
    lived inside the undo layer, while the version gate
    (`persist::FORMAT_VERSION`) lived in the IO layer, so each imported
    the other.
  - `script ↔ signal` — `signal::compile` lowers to the Tier-B kernel,
    which lived under `src/script/`, while `script::bridge` reads the
    signal bus.
  - `interaction ↔ render` — tools drew through
    `render::overlay::OverlayGizmos` while `render::joint_viz` read
    interaction's selection/camera state.
  - `geometry → domain` — the "pure" geometry layer imported
    `domain::shape::ShapeDef`, the very type it is the algorithm layer
    *for*.
- **Boilerplate accretion** at the two seams the project grows through:
  every new intent was registered in three hand-maintained lists
  (`add_message`, `register_type`, a drain-and-box arm in `dispatch`),
  and every new scene-travelling settings resource was listed three
  times in `EnvironmentRecord` (field, capture, apply).

## Target architecture (executed)

One binary/app package at the root plus 13 library packages under
`crates/`, one per architectural layer. The dependency DAG *is* the layer
diagram — a boundary violation is now a compile error, not a test
failure:

```text
gradiance-kernel      pure Tier-B numeric kernel (no bevy, no deps)
gradiance-core        ids, units, constants, states        (bevy minimal)
gradiance-geometry    SDF shape types + all 2D/2.5D math   (bevy minimal, lyon, clipper2)
gradiance-domain      authored components = save content   (avian; core, geometry)
gradiance-physics     collider/joint sync, read facade     (core, domain, geometry)
gradiance-signal      dataflow evaluation                  (kernel, core, domain, physics)
gradiance-scene       records + RON format + migrations    (core, domain)   ← was command::snapshot + half of persist
gradiance-command     intents, GameCommand, undo stack     (scene, signal, …)
gradiance-persist     disk IO, dialogs, autosave           (scene, command)
gradiance-interaction tools, gestures, camera, overlay gizmo groups
gradiance-render      derived meshes/materials/gizmos      (may read interaction editor state)
gradiance-script      steel bridge + registry              (steel confined by Cargo.toml)
gradiance-ui          the egui editor                      (egui stack confined by Cargo.toml)
gradiance (root)      plugin group, prelude, main, tests/it
```

Decisions that shaped the DAG:

1. **`gradiance-scene` is the new home of the save format.** The
   records (`BodyRecord` … `SceneRecord`), `FORMAT_VERSION`, RON
   encode/decode, and version migrations move out of `command`/`persist`
   into one package. The save file is a first-class artifact; both the
   undo layer (records are the unit of undo capture) and the IO layer
   (files are records on disk) depend on it downward. This kills the
   `command ↔ persist` cycle and is the paydown for the serialization
   debt.
2. **The kernel leaves `script`.** `src/script/kernel.rs` was already
   pure ("nothing here depends on steel, bevy, or the ECS"); as
   `gradiance-kernel` it sits at the very bottom, `signal` stops
   importing `script`, and the perf rule ("the VM never runs per-frame;
   drivers lower to the kernel") is now a dependency-graph fact.
3. **`ShapeDef` moves to `geometry`.** The SDF tree is the geometry
   layer's own subject; `domain` re-exports it (`domain::shape`) so the
   authored-component story is unchanged. Geometry keeps a minimal
   `bevy` dependency (`default-features = false`) for the
   `Component`/`Reflect` derives only — no systems, no queries; its
   tests still build without the engine stack.
4. **The overlay gizmo groups move to `interaction`.** They are the
   *interaction-plane* overlay; tools were already their main writers.
   `render → interaction` remains as the one sanctioned upward-looking
   edge (visualizing editor state), now explicit in Cargo.toml instead
   of implicit in a module import.
5. **`egui` and `steel` confinement becomes Cargo metadata.** Only
   `gradiance-ui` declares the egui stack; only `gradiance-script`
   declares `steel-core`. `tests/boundaries.rs` shrinks to the checks
   the package graph cannot express (exact-pin audit).
6. **`CommandStack` privacy becomes compiler-enforced.** The stack is
   `pub(crate)` in `gradiance-command`; nothing outside the package can
   name it. UI keeps reading the `HistoryInfo` mirror.

The root package re-exports every layer under its old module name
(`gradiance::command`, `gradiance::geometry`, …), so the integration
suite (`tests/it/`), the prelude, and every doc link keep working
unchanged.

## Tech-debt paydown folded into the split

**Serialization** (`gradiance-scene`):
- Records, format version, migrations, and RON encode/decode live
  together; adding a v6 migration touches one package.
- `EnvironmentRecord`'s field/capture/apply triplication collapses into
  one declarative macro listing each scene-travelling resource once.
- The triplicated "capture world → serialize → write file" flow in
  `persist` (save / snapshot / autosave) collapses into one
  `write_scene_file` helper, and the two hand-rolled message `drain`
  helpers (dispatch, persist) become one shared utility.

**Undo/redo** (`gradiance-command`):
- The three hand-maintained per-intent lists (message registration, type
  registration, dispatch arm) collapse into one `intents!` table: an
  intent declares how it builds its command (`IntoCommand`), and
  registration + drain + dispatch are generated. Adding a command is now
  one enum-free table row plus the command struct itself.
- `SpawnBodyCommand`/`SpawnNodeCommand` (identical spawn/despawn
  choreography) unify over the record type.

## Feature tree — the roadmap mapped onto the packages

The next milestones from `docs/roadmap.md` ("substrate first, then script
it") land as follows; each feature names the package(s) it grows in, which
is the point of the split — a milestone's blast radius is now visible in
`Cargo.toml` diffs:

1. **Constraints & joints (M20: spring/damper/motor, slider limits,
   gears/pulleys later)** — `gradiance-domain` (new `JointKind`
   variants), `gradiance-physics` (`joint_sync` lowering),
   `gradiance-command` (one `intents!` row each), `gradiance-ui`
   (inspector rows). New physics quantities get a
   `physics::queries` read *as they land* (plotter/scripting gate).
2. **Tracers & plotters** — `gradiance-render` (trails/overlays),
   `gradiance-ui` (dock panes), reading exclusively through
   `gradiance-physics::queries` — no new mutation paths.
3. **Scripting P2 (sensor → modulator → actuator drivers)** —
   `gradiance-signal` (dataflow graph), `gradiance-kernel` (new ops as
   needed, still allocation-free), `gradiance-script` (verbs). The
   package DAG *is* the P2 architecture: sensors = physics reads,
   modulators = kernel, actuators = command intents.
4. **CAD polish (M18 grids/snapping) & rendering/camera (M19)** —
   `gradiance-interaction` and `gradiance-render` respectively, with
   settings staying on the config seam in `gradiance-domain`.
5. **Fluids/particles (roadmap phase 4+)** — a future
   `gradiance-particles` package between `physics` and `render`; bulk
   state is derived-only (never `scene` records), which the DAG now
   states by construction.

## Verification

The gate is unchanged: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`
(workspace-wide), `cargo doc` with `-D warnings`. The integration suite
runs against the root package exactly as before the split.
