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
3. **`avian2d` is imported only inside `src/physics/`.** Everything else uses
   engine-agnostic domain components and the `physics::queries` facade. This preserves
   the option to swap physics engines.
4. **`egui`/`bevy_egui` is imported only inside `src/ui/`.** UI reads component copies
   and emits intents; it never mutates the `World` directly.
5. **Authored vs derived:** components in `src/domain/` (+ `StableId`) are the save file.
   Derived components (colliders, meshes, materials, avian joints) are rebuilt by
   `Changed<>`-driven sync systems and are never serialized, never captured in undo
   records, and never read by commands.

## Conventions

- Identity: `core::ids::StableId` (UUID) on every authored entity; never persist or
  cross-reference raw `Entity`. Joints reference bodies by `StableId`.
- Units: `PIXELS_PER_METER = 100.0`; gravity `(0, -1000)`; polygon vertices are
  centroid-relative; extrusion depth = collision layer bits × `LAYER_HEIGHT = 10.0`
  (bit 0 front … bit 31 back); CSG at `CLIPPER_SCALE = 100_000`.
- `src/geometry/` is pure (no ECS imports) — put all testable math there.
- Errors: `thiserror` enums; no `unwrap`/`expect`/`panic!` outside tests (clippy denies).

## Build & test

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings   # lint gate == CI
cargo test                                   # includes tests/boundaries.rs (layer rules)
cargo run                                    # native app; needs libasound2-dev libudev-dev
```

Every change must leave fmt+clippy+test green; milestones are not done otherwise.
