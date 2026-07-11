# Gradiance

An Algodoo-inspired 2.5D physics sandbox with CAD-grade editing, built on
[Bevy](https://bevy.org) 0.19 and [Avian2d](https://github.com/Jondolf/avian)
physics. 2D bodies extrude into 3D: each collision layer bit maps to a slab
of depth, rendered with toon-banded lighting and cast shadows.

## What it does

- **Draw and simulate**: boxes, circles, freehand polygons, and an infinite
  ground plane, under XPBD physics with pause/step and adjustable speed.
- **SDF geometry**: every shape is a signed-distance tree (see
  `docs/sdf-geometry-decision.md`). The cut tool subtracts a stroke from
  whatever it crosses — connected remainders stay *analytic* (a notched
  circle is still exactly a circle minus a box), severed bodies split into
  independent pieces with joints reattached by anchor.
- **Constraints**: hinges (with limits and oscillating motors), welds, and
  sliders, created by clicking bodies — pin to the world by clicking one
  body. Joints are first-class authored entities: delete/duplicate/undo
  compose with them by construction.
- **CAD editing**: object snapping (vertex/midpoint/center/edge) beating
  configurable Cartesian/isometric/polar grids, axis-constrained drags,
  rotation quantization, scaling by bounding-box handles in global or local
  frames, linear/radial array copies, Ctrl+drag duplication, box and lasso
  selection, grouping, and per-body collision-layer control.
- **Undo everything**: every mutation is a command; one gesture is one undo
  step — including scene loads.
- **Scenes**: RON save files (`Ctrl+S`/`Ctrl+O`), F12 debug snapshots, and
  `gradiance <scene.ron>` to reproduce a session from a snapshot.

## Controls

| Key | Tool |
|---|---|
| `S` | Select / move / rotate (right-drag) / scale (handles) |
| `D` | Drag (mouse spring while simulating) |
| `B` / `C` / `P` | Box / circle / polygon |
| `G` | Ground half-plane (drag to tilt) |
| `H` / `W` / `R` | Hinge / weld / slider |
| `K` | Cut |

`Space` pause · `Ctrl+Z`/`Ctrl+Y` undo/redo · `Ctrl+drag` duplicate ·
`Ctrl+A` select all · `Del` delete · `F` global/local scale frame ·
`X`/`Y` axis locks and `Ctrl` angle-snap while dragging · `Alt`-drag lasso ·
right-click context menu (group, layers, isolate collisions) · `F12`
snapshot. Middle-click any slider in the UI to reset it; numeric fields
accept scientific notation.

## Building

```sh
# Linux build dependencies (clang+lld: fast linking, see .cargo/config.toml)
sudo apt-get install libasound2-dev libudev-dev pkg-config libwayland-dev libxkbcommon-dev clang lld

cargo run --release

# Development inner loop: dynamic linking (fast relinks) + asset hot-reload
cargo run --features dev

# Reopen where you left off: the scene is autosaved on exit
cargo run --features dev -- --resume

# Author from a file: runs at startup and hot-reloads on every save
cargo run --features dev -- --script my-scene.scm
```

Requires Rust ≥ 1.95. Releases are single self-contained binaries (shaders
are embedded). If your machine lacks `lld`, comment out the `rustflags` line
in `.cargo/config.toml`.

## Architecture

One-way dataflow with a single mutation choke point, mechanically enforced
by lints and `tests/boundaries.rs`:

```
tools / UI ──intents──▶ command dispatch ──▶ authored components
                             │                     │ Changed<>
                        CommandStack          derived state
                        (undo/redo)     (colliders, meshes, joints)
```

- `src/domain/` — authored components; exactly what the save file contains.
- `src/geometry/` — pure math (SDF trees, contouring, tessellation,
  extrusion, snapping); no ECS imports.
- `src/physics/` — the only module allowed to import `avian2d`.
- `src/ui/` — the only module allowed to import `egui`.
- `src/command/` — intents, commands, and snapshot records shared by undo
  and persistence.

See `CLAUDE.md` for the full invariants and `docs/bevy19-notes.md` for
verified Bevy 0.19 API notes.

## License

MIT OR Apache-2.0, at your option.
