# Roadmap

Numbers in parentheses reference `docs/feature-feedback.md` items.
Completed rounds run M12–M17.1 plus **scripting P1** (the operation registry,
edit/config/query verbs, the REPL, and `--script` — see `docs/scripting.md`).
Priority now rebalances to the **physics/interaction/visualization substrate
before the scripting that drives it** — read "Sequencing after M17.1" below for
the ordering; M18–M21 and the tracers/plotters milestone carry the detail.

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

## Addressed in M17 (tools rework + scripting substrate)

Built *through* the seams the scripting/symbolic direction requires (see
`docs/script-lisp-decision.md`), so programmability accretes rather than
bolts on later.

- **`ToolContext` facade → `(preview, commit-intent)`**: the pure creation
  tools (box, circle, polygon, ground, cut) now implement one interface,
  `DraftTool` — `update(&ToolContext) -> Option<ToolCommit>` +
  `preview(&ToolContext, &mut ToolPreview)`. A single generic driver
  (`run_draft_tool::<T>` / `draw_draft_preview::<T>`) owns all the
  press/drag/release/gizmo plumbing, so each tool file is just its draft
  state plus decision logic. `ToolCommit` is a front-end-agnostic runtime
  representation of an authoring action that the driver translates into the
  real intent — the *same* intent seam tools already use, so tools keep no
  bespoke mutation path. This makes "a scripted tool is later just a
  closure implementing `DraftTool`" literally true; a pure unit test drives
  `BoxTool` through a hand-built context (no ECS) to lock the contract.
- **Reflection substrate**: `CutIntent` is the first intent to derive
  `Reflect` (all-leaf fields), and the config-seam settings resources
  (`GridSettings`, `SnapConfig`, `SimSettings`, `RenderSettings`,
  `DebugSettings`) plus `CutIntent` are now **registered in the
  `TypeRegistry`**, so the scripting registry can address them by reflected
  name. Config edits continue to flow only through settings resources; the
  `physics::queries` read facade stays the sole simulation-read path.
## Addressed in M17.1 (scripting substrate completed + full tool unification)

The two items M17 deferred are now done (merged), completing the tools/scripting
substrate round.

- **Spike #1 resolved — the full authored intent surface is reflectable.**
  `StableId` (Uuid newtype) and `ShapeDef` (SDF enum) reflect as
  `#[reflect(opaque)]` (FromReflect via `Clone`); everything else reflects
  structurally, including `BodyPhysics` over avian's own `Reflect` components.
  Every authored intent (`SpawnBodyIntent`, `SpawnJointIntent`, …) now derives
  `Reflect` and is **registered in `CommandPlugin`**, so the operation registry
  can bind body/joint ops by reflected name. The generic `bevy_reflect` ↔ steel
  bridge (`src/script/reflect_bridge.rs`, feature-gated) reads a real intent.
  See `docs/script-spike-findings.md` (spike #1 follow-through).
- **Manipulation tools migrated onto the facade.** `select`, `drag`, and the
  connector tools now implement the `ManipTool` half of the tool facade —
  `update(ctx, world, selection) -> ManipOutput` — reading through the
  read-total `ToolWorld` facade and returning commits (→ the same `ToolCommit`
  → intent seam), a kinematic `HoldState`, a mouse-spring `GrabState`, and a
  `SelectTransition`. No tool retains a bespoke intent-writing path: reads are
  total, writes are seam-mediated. This is the seam a scripted or node-editor
  tool reuses.

## Sequencing after M17.1 — substrate first, then script it

Priority rebalance (2026-07-11): build the **physics / interaction /
visualization substrate** — constraints, joints, interactions, tracers,
plotters — *before* the scripting layer that drives or introspects them. The
scripting P1 doorway (edits, config, reads, the operation registry, the REPL,
`--script`) is done and stable; the deep scripting phases (P2 drivers/dataflow,
P3 symbolic) are deliberately **queued behind** the substrate so we script real
features, not placeholders. The accretion rule keeps this cheap rather than a
rewrite-later trap: each substrate feature is built *through* the same seams the
scripting layer binds to, so "script it" is a later derive.

**Execution order** (UI interleaved — a physics feature is *not done* until its
inspector / gizmo / context-menu UI lands, so UI is part of each step, not a
trailing pass):

1. **Interactions & joints/constraints** (M16 residual + M20). The core sim
   substrate and its editing UI: selection-from-any-tool and shift-drag
   semantics, play-mode torque, joint config in the context menu, then the
   weld/spring/damper/motor and contact-force work of M20.
2. **Tracers & plotters** (milestone below). Live physics introspection —
   trajectories, time-series, probes — enabled purely by the read-total facade.
   UI-heavy (overlays + dockable plot panels).
3. **Script the above — scripting P2** (drivers as sensor/modulator/actuator
   dataflow), now architecturally paid for: a **sensor** is a read over the
   facade (step 2's readers), a **modulator** is a Tier-B `kernel`, an
   **actuator** is a registered edit/config op (step 1's intents). Then P3
   (symbolic field forces) over the SDF substrate.

CAD polish (M18 grids/snapping) and rendering/camera (M19) interleave where they
unblock the above — snapping aids constraint assembly; the M19 sim-settings UI
pairs with the constraints work.

**Architectural-readiness gates — we are ready; the gate is discipline, not a
missing foundation:**

- *Plotters/tracers* need the read-total facade (`physics::queries` + the
  scripting `SceneView`). It exists — the rule is **keep it complete**: every
  new physics quantity (constraint force, contact point, joint error) gets a
  query *as it lands*, so plotters and scripts read it for free.
- *Constraints-as-script* need each new constraint/joint intent to derive
  `Reflect` and register in `CommandPlugin` — the established pattern (all
  authored intents already do). Do it as each constraint type lands and the
  operation registry binds it with no bespoke code.
- *Drivers/dataflow (P2)* need the read facade (sensors), the Tier-B kernel
  (modulators — done, `src/script/kernel.rs`), and the operation registry
  (actuators — done). Unblocked the moment step-1/2 quantities are queryable.

## M16 residual / deferred (Algodoo parity, queued)

- Selection works from every tool (click falls through to select) (2.1) —
  **landed**: `tools/click_select.rs` applies a sub-deadzone click no tool
  consumed (no commit intent, no live draft) through the same
  `SelectTransition` seam, in every tool state.
- Shift-drag / modifier semantics rework; no gesture dead-ends (1.1, 2.2) —
  **landed**: `Shift` is selection-only (click = toggle, drag from a body =
  additive rubber band via the `ShiftPick` state — never a move, never an
  axis warp); the dominant-axis lock moved to `X`+`Y`. The selection outline
  now draws front-biased (own gizmo group), visible above prisms and grid.
- Play-mode right-drag applies torque (dynamic rotate, non-fixed pivot) (2.6)
  — **landed**: while playing, the rotate gesture servos each selected body's
  angular velocity toward the gesture angle (`physics::grab::MouseTwist`,
  the angular sibling of the drag spring) instead of kinematic-holding poses;
  translation stays with the solver so a resting body lifts its opposing
  edge. Physical interaction — no command, not undoable, like the drag grab.
- Inspector re-architecture: context-menu-first, inspector as pop-out (2.8)
  — **landed**: the property sections are host-agnostic renderers
  (`ui/inspector.rs`) shared by the right-click menu (Material/Appearance
  collapsing sections + "Properties…" command) and the *Properties* pop-out,
  which is closed by default and opened from the menu or the transport
  toggle. Right-click now selects the clicked body when it isn't already in
  the selection, so the menu always binds to what was clicked.
- Collision-layer set visualization UI (5.4) — **landed**: the shared
  layers section is now one grid — color-swatch + scene-occupancy header,
  a **member** row (occupancy = render depth) and a **hits** row (collision
  filters) — plus a `DebugSettings.show_layers` viewport overlay that
  outlines every body in its front-most layer's hue (`layer_hue` is shared
  by the UI swatches and the overlay, so colors always match).
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

## M18 — Grids & snapping (CAD pass)

- Major/minor grid lines; snap points provably on the grid at every
  adaptive zoom level (3.3, 2.5)
- Axis-lock basis follows the active grid system (2.5)
- Light collinear/centerline snapping while dragging ("lightweight
  assemblies") — configurable in the snapping menu (3.1)
- Alternate constructors: 3-point box, tangent circle (3.1)
- Curvilinear abstraction: tools operate in grid coordinates (polygon
  edges curve in polar grids) (3.3)
- Snap glyph stability, tangent glyph, snap-off-when-grid-hidden (3.4)

## V — Visuals & simulation (active branch: claude/visuals-roadmap)

The visuals/sim track, sequenced as one PR per slice. Absorbs the AO/contact
shadow items from M19. Design decisions in the slice PRs.

- **V1 — Interaction plane, back plane, lighting** *(landed)*: one
  `OverlayGizmos` group with the grid moved to the picking plane (kills the
  grid parallax, 3 gizmo groups → 1); back plane + key light driven by new
  persisted `ScenerySettings`/`LightingSettings` (Lighting tab with a
  draggable sun gadget, SSAO + contact-shadow toggles); half-plane grounds
  render effectively infinite with an inside-view fade material.
- **V1.5 — Visual-feedback pass** *(landed)*: hard shadows (single tight
  cascade + configurable shadow-map size/reach), up to 4 key lights for
  colored shadows, grid demoted to its own transparent gizmo stratum below
  overlays, ground/back plane unified as one infinite-plane material
  (orientation is the only visual difference), configurable perspective
  projection (`ScenerySettings::perspective_deg`) for depth parallax, and
  a `CameraScale` resource as the single screen↔world sizing authority.
  Second pass (scene readability): planes render opaque with a horizon
  fog + dithered inside-reveal (no transparent-sort flips, crisp plane
  seams), a faint authoring-plane trace line on every tilted plane, an
  orthographic depth slab sized past the plane quads (no frustum cuts),
  fully matte bodies + quantized specular glint (view-stable banding),
  free orbit (yaw wraps, pitch to the poles) with view-plane panning,
  and back/ground plane visibility in the background context menu.
- **V2 — View cube + top-down mode** *(landed)*: CAD orientation cube
  (face/corner/edge snaps, animated glide), `CameraRig` roll, and a
  top-down preset — gravity (0,0) plus a per-scene back-plane friction
  force (`physics/forces.rs` sibling system, Coulomb μ·m·g with a
  gyration-radius spin term), set from a right-click "Background" menu
  section.
- **V3 — Continuous-depth collision** *(landed, save v5)*: authored
  `DepthBand {near, far}` replaces `LayerMask32`; contiguous collision
  layers and the exact extrusion both derive from the band (visual depth ≡
  collision layer by construction; non-integer depths allowed, quarter-layer
  UI snapping avoids slivers; ground half-planes collide with all). v4
  files migrate on load (masks → bands; custom filters dropped with a
  warning). Right-dock Depth panel: selected bodies as draggable colored
  bars (edges resize, middle moves, auto-growing bounds, one intent per
  drag); the checkbox grids, layer buttons, and depth-shift-by-bit menu
  are deleted. Deferred: a "no self-collisions within selection" escape
  (was filter art; would return as an authored flag + collision hook if
  needed).
- **V4 — Script dock & workspace** *(landed)*: console became the right
  dock (shared host with the Depth panel) with MATLAB REPL behavior —
  Enter runs, Shift+Enter newline, ↑/↓ prefix-filtered history, each run's
  value echoed and bound to `ans`. Spawn verbs return body handles; the
  `label` verb gives bodies workspace names rendered as viewport tags and
  in the context-menu pick list (StableId underneath).
- Deferred within this track: local-frame grids, ground dot-grid, gradient
  pickers, color-by-signal (pairs with P2 dataflow), MPM/fluids/fracture
  (split a `gradiance-sim` crate when that work starts).

## M19 — Rendering & camera polish

- Emissive material option; ambient occlusion / contact shadows for the
  clay-matte look (9.3)
- Body borders: default dark-gray outline, per-body border color and
  transparency via context menu (1.5)
- Themed default palette; quantized color picker; random colors within a
  grouped selection (1.5)
- Camera settings section (zoom sensitivity etc.) (10.2)
- Sim-settings UI: scrub-drag values, gravity direction widget (8.2)

## M20 — Constraints II

- Joint limit handles (user request, 2026-07-12) — **landed**: hinge and
  prismatic limits are draggable handles on the glyph itself (hinge: an
  exact allowed-rotation arc with end handles; prismatic: the travel caps),
  with a live tentative preview and one undoable `PropertyEditIntent` per
  drag. Joints are selectable **anywhere on their glyph** (ring / travel
  line / spring coil — shared `glyph_distance` drives both left-click and
  right-click picking). The slider is labeled *Prismatic* in the UI.

- Weld rework: merge bodies into one (SDF `Union` — the tree makes this
  natural) or make-static, replacing the weld-as-joint model (4.2) —
  **landed**: the weld tool merges the two topmost bodies at the click
  (`MergeIntent`) or pins a lone body by making it static (a
  `PropertyEditIntent`); `JointKind::Weld` is removed everywhere
  (`FORMAT_VERSION` → 3; the infinite ground is never a weld target —
  welding onto it *is* the make-static case).
- Slider default limits option; sprite-based joint glyphs with outlines
  (4.7); motor state (direction/torque) visualization (4.1) — **landed**
  (sans sprites): new sliders default their travel to `[0, drag length]`
  (the drag *draws* the travel; `ToolDefaults.slider_limits` toggle in
  Grid & Snap → Tools), the slider glyph renders the actual travel span
  with end caps, glyphs are grey with a dark under-stroke, and motor
  arrows scale with drive speed (direction *and* magnitude). Sprite-based
  glyphs remain open.
- Springs/dampers — **landed** as the **strut** tool (`JointKind::Spring`
  over avian's `DistanceJoint` + `JointDamping`): drag from one anchor to the
  other (drag length = rest length), configurable length bounds, spring
  constant, and damping; drawn as a non-colliding spring-coil gizmo. The three
  scalar knobs are the ones a future curve editor would let vary nonlinearly.
- Cams, planar contact, magnetism (SDF force fields), breaking limits,
  backlash (12) — **fields landed** (Algodoo attraction): authored
  `FieldSource` (signed repulsion, negative attracts; Linear/Quadratic
  falloff — Algodoo's exact menu) acting on *every* dynamic body,
  mass-scaled. One sampling cut-point (`physics::fields::Fields::accel_at`,
  see `docs/field-architecture.md`) serves the solver forces, the
  vector-plot overlay (`show_fields`), and **"Set in orbit"** (context
  menu: `v = √(a·r)` about the dominant attractor). SDF-shaped by default
  (surface distance + gradient). Cams/planar contact/breaking/backlash
  open.
- Contact point & force debug overlays (2.6, 8.3) — **first cut landed**:
  `PhysicsQueries::contact_points()` reads avian's `ContactGraph` (read-only, so
  the facade stays complete for plotters/scripts), and `DebugSettings.show_contacts`
  draws each contact point and its impulse-scaled normal.
- Engine tuning: timestep/substeps in Simulation settings, substep debug
  view (8.3) — **landed**: `SimSettings.timestep_hz` (15–240 Hz, applied to
  the fixed clock alongside the existing substeps knob; both appear in the
  Simulation tab via the reflect grid) and `DebugSettings.show_substeps`, a
  debug overlay tracing every dynamic body's position at each solver
  substep of the last step (`SubstepTrace`, recorded inside avian's
  `SubstepSchedule`).

## Tracers, plotters & live probes (read-facade visualization)

The visualization half of the constraints/joints work, and step 2 of the
sequencing above. Enabled entirely by read-total governance — a plotter or
tracer is *just another reader* of `physics::queries` / the scripting
`SceneView`; no new mutation, no persistence, no invariant exception (see
`docs/script-lisp-decision.md` §"Live plotters are enabled by read-total
governance").

- Body/point **tracers**: sampled trajectories drawn as fading polylines. A
  tracer is an authored marker (one op); its samples are *derived*, never
  command-wrapped or serialized (rule #5).
- **Live plotters** — **landed** (`src/ui/plot.rs`): a backslash-toggled panel
  time-series-plots the selected **body** (speed, height) *or* selected **joint**
  (length, and a hinge's angle), sampled each frame while playing from
  `physics::queries` (a pure read; a pause freezes the plot). The history is a
  generic **named-signal** store, so adding a plottable quantity is one line and
  the model is ready to back the `(measure …)` data-out seam. Hand-drawn with
  the egui painter, no plotting dependency. Next: contact-force signal, pinnable
  multi-body probes, and the script-driven `(measure …)` seam.
- **Probes**: hover or pin a body to read its live physics (velocity, forces,
  sleep state) — the read facade surfaced in the UI.
- Discipline it enforces: each new plottable quantity is *added to the query
  surface*, which is exactly what makes it scriptable (a sensor) for free.

## M21 — CSG modeling & pieces

- Boolean operations between bodies via context menu (join / subtract /
  intersect / xor) producing analytic trees (7.3)
- Piece velocity inheritance `v + ω × r` on severing cuts (7.5)
- Smooth-union (fillet) modeling tools (12)

## Scripting & symbolic modeling (design accepted)

Direction is ratified in `docs/script-lisp-decision.md`: a Lisp/DSL over a
governed, homoiconic operation registry as the tool's control plane, with a
two-tier execution model (authoring VM cold; compiled numeric kernels hot).
Programmability is not one milestone — it *accretes through* M16–M17 (tool
facade, settings/grid ops, `Reflect` derives) per the decision record.

**Both linchpin spikes have passed (`docs/script-spike-findings.md`), and the
substrate they gated is now in place** — so feature phases P0/P1 are unblocked:

- **Spike 2 (perf) — done.** `src/script/kernel.rs`: numeric DSL → flat
  allocation-free tape, VM-free hot-path eval over SoA columns; proptested,
  ~27.7 M evals/s (debug) at particle scale. De-risks the fluid/particle ceiling.
- **Spike 1 (reflect ↔ steel bridge) — done.** `src/script/reflect_bridge.rs`:
  one generic converter reads/writes any `#[derive(Reflect)]` value by
  reflect-path. Spike #1 follow-through (M17.1) then made the **whole authored
  intent surface** reflectable + registered, and the bridge reads a real intent.
  The "everything programmable" endgame is a derive, not N builtins.

Feature phasing (`docs/script-lisp-decision.md` §"Feature-level phasing"):

**Scripting is now a first-class, always-on part of the tool** (`steel-core` is
a normal dependency, not a cargo feature — see the decision doc's
"Direction update"). The two-tier PERF rule still holds (VM cold-path only).

- **P0 — substrate: done.** `kernel.rs`, `reflect_bridge.rs`, the reflect
  substrate (opaque `StableId`/`ShapeDef`, registered intents), `ScriptError`
  (thiserror) + a `catch_unwind` eval boundary, and `ScriptPlugin`. Remaining
  P0 nicety: `src/script/values.rs` (`ShapeDef` foreign type + geometry
  constructor builtins) and a fuel/step budget.
- **P1 — scripted editing + reads: landed.** `src/script/bridge.rs` runs the
  exclusive `run_scripts` doorway (before `CommandDispatchSet`, so one run =
  one batch of undoable commands), and both halves of the governance model are
  now concrete:
  - **Operation registry (the homoiconic spine).** `src/script/registry.rs`
    is a pure `OperationCatalog` — `OpSpec` metadata (name, signature, doc,
    governance category) for every verb, keyed by shared `name` constants so
    the catalog and the steel registration cannot drift. Surfaced as the
    `OperationRegistry` resource that the console (highlighting, completion,
    reference panel) reads, and that data-driven menus / user tools will bind
    to. Introspectable from inside the VM via `(ops)` / `(describe …)`.
  - **Edits (writes → intents).** `spawn-box`, `spawn-circle`, `cut`; each
    builtin emits a reflected intent, `IntentDispatch` binds it to its bus by
    type. No new mutation path.
  - **Config (writes → settings resources).** `sim-get` / `sim-set` name any
    `SimSettings` scalar by reflect-path (the spike-#1 bridge doing exactly its
    job — no per-field builtin) and apply it through the invariant-#4 settings
    seam, never the command stack. Completes the Edit/Config/Query triad.
  - **Reads (total, seam-free).** A per-run `SceneView` snapshot feeds
    geometric-query builtins — `body-count`, `body-x/y/rot`, `count-at` (exact
    SDF containment), `nearest-at` — so a script observes committed scene state
    without ever holding `&World`. This is the read facade live plotters reuse.
  - **REPL panel** (`src/ui/console.rs`): a backquote-toggled lisp editor with
    registry-driven highlighting/completion, an output log, and a reference
    panel; submits to the same `ScriptInputs` queue tests and files use.
  - **`--script foo.scm` loader** (`StartupScripts` + a `Startup` system): CLI
    or resource-supplied `.scm` files are read and run through the ordinary
    doorway on boot — the "author from a file" path (scene setup, helpers,
    `register-action`). Doubles as a test-fixture runner.
  - **Remaining P1:** a fuel/step budget for runaway authoring scripts.
- **P2 — drivers as named-signal dataflow** over the Tier-B `kernel` seam
  (`defsignal`/`defparam` → auto-slider; live probes / tracers / plots).
  **Queued behind the physics/tracer substrate** (see "Sequencing after M17.1"):
  we script drivers once the constraints/joints and the tracer/plotter read
  surface they drive and observe are in place. The seams are ready — a **sensor**
  is a read (a query builtin / reflect read over the facade), a **modulator** is
  a `kernel` over signals, an **actuator** is a config- or edit-op the signal
  drives — so P2 is a wiring layer, not new foundation.
- **P3 — the extension surface (the tool authored in its own DSL).** Because
  edits, config, and reads all route through one registry, the editor's own
  chrome can be *authored as data over that registry*:
  - **Data-driven context-menu actions (first cut landed).** `register-action`
    (the `EditorState` seam) lets a `.scm` add a labelled action to the
    `ScriptActions` table; the context menu's "Scripts" section surfaces them
    and invoking one runs its source through `ScriptInputs`. Next: pass the
    click point / selection into the action, and fold the existing hard-coded
    `src/ui/context_menu.rs` buttons into the same table as the built-in seed.
  - **User tools from `.scm`:** a tool is a `ToolContext → (preview,
    commit-intent)` (M17's `DraftTool`) / `ManipTool` closure; the registry
    lets one be authored in lisp and registered by name, reusing the exact
    press/drag/release driver the built-in tools use.
  - **Targeted symbolic ops** (`grad`/`solve`, symbolic **field forces** over
    the SDF substrate — the flagship demo). Optional parametric `.scm` scene
    export (RON stays canonical regardless).

The former backlog lines below are now subsumed by that record:

## Backlog / later

- Curve pickers (lightroom-style), symbolic & equation input — see the scripting
  section above and `docs/script-lisp-decision.md` (12)
- Tracers / live plotters, scripting, fluids — enabled by the read-total facade
  and Tier-B kernels in the decision record (12)
- Investigate: load-time crash reported with a pre-M12 partial-cut save
  (11.1) — cuts no longer produce those trees, but saved ones must render;
  add a Csg tessellation robustness proptest when touching the mesher.
