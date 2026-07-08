# Roadmap

Numbers in parentheses reference `docs/feature-feedback.md` items.
Updated after the M12 feedback round.

## Addressed in M12 (this round)

- **Joint rest frames** (4.3, partially 4.2): joints capture creation-time
  rotations; sliders lock rotation at the authored angle instead of
  snapping upright and exploding (regression test distilled from
  `snapshots/gradiance-1783344618.ron`); welds hold rotated pairs at their
  authored relative angle.
- **Cut severs-only** (7.3): partial strokes are rejected outright — no
  notches, no thin features, no undo entry.
- **CAD camera** (9.3, 9.4 foundation): middle-drag orbits, `Home` glides
  back to 2D, and picking is ray/plane — the tilted view is a real editing
  view (replaces the broken Tab peek). Extrusion and rim light are only
  visible when orbited; head-on stays nominally 2D by design.
- **Selection** (2.2, 2.3, 2.4, 1.4): box select takes only fully-contained
  bodies and never the ground; lasso moved to Ctrl+drag (Alt is
  OS-reserved; Alt still works where free) and skips the ground;
  duplicated/arrayed groups get fresh group ids (the "deteriorating group
  selection" bug); rotate deadzone widened (5.1).
- **Ground** (1.4 partial): Ctrl quantizes the tilt during creation;
  excluded from box/lasso selection.
- **Colors** (1.5 partial): new bodies get a random pleasant hue derived
  from their id (no more all-red).
- **Layering** (1.3, 2.2): grid renders behind bodies and ghosts;
  selection outlines render in front of everything.
- **Misc**: circle center-to-edge radius line (1.2); joint glyphs scale
  with zoom (4.7 partial); gentler zoom (10.2 partial); context-menu
  renames "No self-collisions" / "Reset collision layers" (5.5, 5.6).

## Addressed in M14

- **Hierarchical groups** (2.9): `SelectionGroup` is now a stack;
  `ungroup(group(group(A,B,C), D))` peels only the outer group and keeps
  `group(A,B,C)`. Selection expands by the outermost id; duplicate/array
  remap every stack level. Old saves migrate (single id → one-deep stack).
- **Documentation overhaul**: crate-level architecture docs with a
  dataflow diagram + module map; per-module docs on the command layer
  (lifecycle diagram + "add a command" recipe) and the SDF core; runnable
  **doctests** on the pure APIs (`PosRot`, `LayerMask32`, `layer_z_range`,
  `sdf::eval`, `SelectionGroup`); `docs/architecture.md` with mermaid
  diagrams (dataflow, command lifecycle, layer boundaries, SDF pipeline,
  depth mapping). CI now builds docs with `-D warnings` (no broken links).

## Addressed in M15

- **Selectable joints** (2.8, 4.1, 4.5): clicking a joint's anchor glyph
  selects it (clearing the body selection). A selected joint shows the
  **joint inspector** — kind, connected bodies, `collide_connected`,
  limits (angle/travel), and motor (target velocity, max effort,
  oscillate, powered) — editing through the same undoable
  `PropertyEditIntent`/`PropertyValue::Joint` path. Delete removes just
  the joint (undoable, no body cascade). In pause mode the anchor drags
  to relocate the joint (one undoable move). The selected joint is
  ringed, and a powered motor draws a direction arrow (curved for hinges,
  straight for sliders) whose length tracks target velocity — the "motor
  state" visualization requested in 4.1.
- **Hinge-vs-weld re-diagnosis tooling** (4.1): the joint inspector now
  labels the kind unambiguously ("Hinge (revolute)" / "Weld (fixed)"),
  so clicking the joint in a live scene shows exactly what it is. The
  headless swing-vs-rigid contrast test still passes.

## Addressed in M16 (interaction foundation + fixes)

- **Selection state machine** (2.2, 2.3 hardening): all selection changes
  now go through one function, `SelectTransition::apply`, which is the
  transition alphabet of a small FSM (`SetBodies`/`AddBodies`/
  `ToggleBodies`/`SelectJoint`/`DeselectJoint`/`Clear`). It enforces the
  **joint-xor-bodies invariant** (body and joint selection are never both
  populated) and group expansion in exactly one place, so every call site
  — select tool, joint pick, context menu "select from stack", Ctrl+A,
  Escape — is a one-line transition instead of ad-hoc resource pokes. This
  removes the class of "both selections live at once" glitches behind the
  stability smell (the lingering-joint bug). New interactions extend the
  vocabulary rather than touching the resources directly. Module docs carry
  the state diagram; a regression test in `tests/interaction.rs` locks the
  invariant.
- **Rendering fixes** (from feedback): the prism **front face is visible**
  again (base material is `double_sided` / `cull_mode: None` so the
  lyon-wound front cap isn't back-face culled); the **backdrop no longer
  clips on tilt** (orthographic near/far widened to a ±50 000 slab so the
  far plane clears the layer stack); and there is a **re-home control** —
  a "⌂ 2D view" transport button (enabled only when tilted) alongside the
  existing `Home` key, both routed through `CameraRig::homing`.
- **Bevy-features audit** (recurring check): confirmed idiomatic use of the
  high-value 0.19 features — component **hooks** maintain `IdIndex`,
  `Changed<>`-driven sync rebuilds all derived data, `States` + `run_if`
  gate tool systems, an exclusive dispatcher drains `Messages<Intent>`,
  and `SystemParam`/`ParamSet` bundles keep systems under the arg limit.
  Two divergences from the original plan are deferred as issues (below),
  each because closing them now would add churn out of proportion to the
  benefit.

## M16 residual / deferred (Algodoo parity, queued)

- Selection works from every tool (click falls through to select) (2.1)
- Shift-drag / modifier semantics rework; no gesture dead-ends (1.1, 2.2)
- Play-mode right-drag applies torque (dynamic rotate, non-fixed pivot) (2.6)
- Inspector re-architecture: context-menu-first, inspector as pop-out (2.8)
- Collision-layer set visualization UI (5.4)
- Joint config also reachable via right-click context menu (not only the
  select-and-inspect flow landed in M15) (2.8)
- **Deferred (Bevy audit):** migrate tool gestures from polling
  `PointerButtons`/`SnappedCursor` to first-party `bevy_picking` observers
  (`On<Pointer<…>>`), as the original plan intended — large surface, best
  done as its own pass so the drag contract is ported wholesale.
- **Deferred (Bevy audit):** enforce the authored `Body`/`Joint` archetype
  with **required components** (`#[require(...)]`) once the domain types
  carry sensible `Default`s (`ShapeDef` is a `Default`-less enum today), so
  a body can never exist missing `LayerMask32`/`Appearance`/etc.

## M17 — Grids & snapping (CAD pass)

- Major/minor grid lines; snap points provably on the grid at every
  adaptive zoom level (3.3, 2.5)
- Axis-lock basis follows the active grid system (2.5)
- Light collinear/centerline snapping while dragging ("lightweight
  assemblies") — configurable in the snapping menu (3.1)
- Alternate constructors: 3-point box, tangent circle (3.1)
- Curvilinear abstraction: tools operate in grid coordinates (polygon
  edges curve in polar grids) (3.3)
- Snap glyph stability, tangent glyph, snap-off-when-grid-hidden (3.4)

## M18 — Rendering & camera polish

- Emissive material option; ambient occlusion / contact shadows for the
  clay-matte look (9.3)
- Body borders: default dark-gray outline, per-body border color and
  transparency via context menu (1.5)
- Themed default palette; quantized color picker; random colors within a
  grouped selection (1.5)
- Camera settings section (zoom sensitivity etc.) (10.2)
- Sim-settings UI: scrub-drag values, gravity direction widget (8.2)

## M19 — Constraints II

- Weld rework: merge bodies into one (SDF `Union` — the tree makes this
  natural) or make-static, replacing the weld-as-joint model (4.2)
- Slider default limits option; sprite-based joint glyphs with outlines
  (4.7); motor state (direction/torque) visualization (4.1)
- Springs/dampers, cams, planar contact, magnetism (SDF force fields),
  breaking limits, backlash (12)
- Contact point & force debug overlays (2.6, 8.3)
- Engine tuning: timestep/substeps in Simulation settings, substep debug
  view (8.3)

## M20 — CSG modeling & pieces

- Boolean operations between bodies via context menu (join / subtract /
  intersect / xor) producing analytic trees (7.3)
- Piece velocity inheritance `v + ω × r` on severing cuts (7.5)
- Smooth-union (fillet) modeling tools (12)

## Scripting & symbolic modeling (design accepted)

Direction is ratified in `docs/script-lisp-decision.md`: a Lisp/DSL over a
governed, homoiconic operation registry as the tool's control plane, with a
two-tier execution model (authoring VM cold; compiled numeric kernels hot).
Programmability is not one milestone — it *accretes through* M16–M18 (tool
`ToolContext` shape, settings/grid ops, `Reflect` derives) per the decision
record. Gated on two linchpin spikes before feature code lands:

- **Spike 2 (perf) — done.** `src/script/kernel.rs`: numeric DSL → flat
  allocation-free tape, VM-free hot-path eval over SoA columns; proptested,
  ~27.7 M evals/s (debug) at particle scale. De-risks the fluid/particle ceiling.
- **Spike 1 (reflect↔steel bridge) — pending.** The linchpin for low-boilerplate
  "everything programmable"; must run before embedding steel.

The former backlog lines below are now subsumed by that record:

## Backlog / later

- Curve pickers (lightroom-style), symbolic & equation input — see the scripting
  section above and `docs/script-lisp-decision.md` (12)
- Tracers / live plotters, scripting, fluids — enabled by the read-total facade
  and Tier-B kernels in the decision record (12)
- Investigate: load-time crash reported with a pre-M12 partial-cut save
  (11.1) — cuts no longer produce those trees, but saved ones must render;
  add a Csg tessellation robustness proptest when touching the mesher.
