# Gradiance Roadmap — the feature tree

The milestone-by-milestone log (what landed, in which round, and why) lives
in `docs/roadmap.md`; architecture rationale and the roadmap→package
mapping live in `docs/workspace-plan.md`. This file is the curated **feature
tree**: what the tool grows next, organized by the workspace package(s)
each feature lands in — since the workspace split, a feature's blast
radius is its branch of the package graph.

Execution order (set 2026-07-11, unchanged by the split): **substrate
first, then script it** — (1) constraints/joints + their editing UI,
(2) tracers/plotters, (3) scripting P2 drivers over both, with CAD polish
(M18) and rendering/camera (M19) interleaved where they unblock the above.

## 1. Physics & constraints — `domain` + `physics` + `command` + `ui`

Each new joint kind = a `JointKind` variant (domain), a `joint_sync`
lowering (physics), one `command_intents!` row if it needs a new intent
(command), inspector rows (ui), and a `physics::queries` read *as it
lands* (the plotter/scripting gate).

- [x] Hinge (revolute) and fixed pin, with limits and motors
- [x] Strut (spring / rigid rod), authored as rest length + optional range
- [x] Play-mode grab forces (mouse spring + twist)
- [x] Attraction/repulsion field sources (`FieldSource`, sampled via `physics::fields`)
- [ ] Slider (prismatic) travel limits polish + motor parity with hinges
- [ ] Damper / damping control on every joint (`JointDamping` is derived today)
- [ ] Gear constraint (angular velocity ratio between two hinges)
- [ ] Rope / pulley (non-colliding length constraint; physically correct pulleys)
- [ ] Chain tool (procedurally generated linked bodies)
- [ ] Contact/force sensors surfaced as signal sources
- [ ] Fluids & particles (SPH, buoyancy, liquify) — future
      `gradiance-particles` package between `physics` and `render`;
      bulk state is derived-only (never scene records) by construction

## 2. Geometry & tools — `geometry` + `interaction` (+ `command`)

- [x] Box / circle / polygon / ground draft tools over the `DraftTool` facade
- [x] CSG cut (severs-only) and merge as SDF tree nodes
- [x] Scale/rotate handles with global/local frames; linear + radial array
- [x] Object + grid snapping (Cartesian / isometric / polar grid systems)
- [x] Box & lasso selection, hierarchical groups, click-through selection
- [ ] Brush/sketch tool: freehand → simplify → polygon (RDP over the SDF substrate)
- [ ] Infinite ground plane (currently approximated; needs an infinite shader — with `render`)
- [ ] Copy/paste shortcuts (Ctrl+drag duplicate exists; clipboard semantics pending)
- [ ] SVG import (drag-and-drop) — much later
- [ ] CAD-style constraint-based sketching — much later

## 3. Tracers, plotters & signals — `signal` + `render` + `ui`

The read-total substrate: everything here reads through
`physics::queries` and the signal bus; no new mutation paths.

- [x] Tracers on bodies and free/attached behavior nodes (fading trails)
- [x] Signal dataflow: source→sink bindings, `defparam` knobs, `defsignal`
      modulators compiled to the Tier-B kernel (`docs/signal-dataflow.md`)
- [x] Live plot panel + probes; node-graph canvas for the signal wiring
- [ ] More signal sources as physics lands them (constraint force, contact
      impulse, joint error) — keep the facade complete
- [ ] Arbitrary coordinate frames for plots (requires frames work)
- [ ] Performance monitor (FPS / body count) as ordinary signals

## 4. Scripting — `script` + `kernel` (P2/P3, queued behind 1–3)

Architecturally paid for: a **sensor** is a `physics::queries` read, a
**modulator** is a compiled kernel, an **actuator** is a registered
edit/config op. The package DAG states the perf rule (`signal → kernel`,
never `signal → script`).

- [x] P1: operation registry, edit/config/query/editor verbs, REPL console,
      `--script` startup loader, `register-action` context-menu hooks
- [ ] P2: driver dataflow (sensor → modulator → actuator) authored in-language
- [ ] P3: symbolic field forces over the SDF substrate
- [ ] Entity event hooks (`on_hit`, `on_spawn`, `on_click`)

## 5. Rendering & UX — `render` + `ui`

- [x] Toon-shaded extruded prisms; continuous depth bands ≡ collision layers
- [x] Dockable egui shell (tiles), tool palette, transport, view cube,
      context menus, inspector, outliner, depth dock
- [x] Scene lighting + scenery settings (key light, ambient, SSAO, back plane)
- [x] Camera: pan/zoom, CAD orbit with 2D re-home
- [ ] Translucency (RGBA alpha blending through the toon pipeline)
- [ ] Lasers / optics (transmission & reflection) — after collision-layer optics design
- [ ] 2D shadow casting / point lights
- [ ] Tweening/animation polish for editor transitions
- [ ] Algodoo-style icon/UI theming pass

## 6. Persistence & system — `scene` + `persist`

- [x] RON save/load with format versioning + v4→v5 migration; undoable load
- [x] F12 debug snapshots; exit autosave + `--resume`; CLI scene loading
- [x] Undo/redo command stack (records shared with the save format)
- [ ] Autosave-on-interval (only exit autosave today)
- [ ] Scene thumbnails / metadata for a future open-dialog gallery

## Cross-cutting: engineering units (SI) — `docs/units-decision.md`

A workspace-wide pass making every physical quantity a typed SI value
(reflection-native newtypes in a new `gradiance-units` crate), with a
single px↔SI seam (`core::world`) and a `PhysicalQuantity` catalog that
sensors, plotters, and scripts share. Persistence revs to **v6** to store
SI. Density stays **2D areal** (kg/m²) — built for a future 3D jump behind
one `mass_of` seam, but no physics-behaviour change now. Unblocks the
units-aware pieces of material-property editing (§1) and plotters (§3).

- [ ] P0 reflection spike + bridge newtype-unwrap (settles reflection first)
- [ ] P1 `gradiance-units` + `core::world` seam (confine `PIXELS_PER_METER`)
- [ ] P2 retype geometry/physics/domain; `physics::queries` returns quantities
- [ ] P3 `PhysicalQuantity` catalog + signal/plotter binding (unit axes)
- [ ] P4 inspector/settings SI display + input
- [ ] P5 scene format v6 (SI-stored) + units-only v5→v6 migration
- [ ] P6 units in the parameter-linking / expression DSL
- [ ] (dedicated path) arbitrary coordinate frames — `docs/frames-decision.md`,
      door held open by the frame-agnostic catalog + `WorldScale` seam

## Maintenance

- Architecture contract: `CLAUDE.md` (package-fenced invariants).
- Known-smell record + kept-pattern rationale: `docs/desmell-log.md`.
- Coupling re-check: run `cargo coupling --ai --exclude-tests` after a
  milestone lands; the workspace split reset the baseline
  (`docs/workspace-plan.md` has the pre-split numbers).
