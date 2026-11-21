# State — where the project is right now

Overwritten at every handoff; history lives in git and in each brief's
Outcome (`docs/briefs/`). Keep it to about a screen.
Last updated: 2026-11-20 (G-0 plan handoff; before that J-2 handoff; before that P-1 perf handoff: islands/sleeping re-enabled with wake seams, per-frame O(N) costs cached/gated, fixed-catchup clamp — brief `docs/briefs/P-1-concurrent-objects.md`, branch `perf/max-concurrent-objects`; before that 6-3 handoff, Phase 6 complete; before that 6-2; before that 6-1, N-9, Phase N complete; before that N-8, N-7, N-6, N-5, N-4, N-3, N-2, N-1; before that R-4, Phase R complete; before that R-3, R-2, R-1; before that, the phase gate after the stack #257–#264, main
3f211ab; six review answers ingested; order per
`docs/unified-surface-plan.md` § 10 and § 13).

## Position

- **Done:** Phases 0–4 (M18 grids/snap, constraints, scripting & op surface,
  CSG booleans/fillet, M19 render + chrome). Phase 2's backlash, cams and
  planar contact are still undefined (retained below). PRs #212–#235. Each
  brief's Outcome records its decisions; read it before touching that area.
- **Now:** **G-0 one graph model: plan written** (2026-11-20, docs only;
  merged #296). Decision
  (`docs/graph-model-decision.md`, `graph-model` in `archive.ron`,
  session best estimate): **one IR, two node families**. A pure
  `gradiance-graph` crate holds nodes by `StableId`, typed ports and edges
  by id; the signal family runs at frame rate (lowers to the kernel), the
  shape family at edit rate (lowers to `ShapeDef`); an edit-rate input
  never takes a frame-rate output. Bodies and joints stay authored
  entities (reference nodes); the graph is scene content edited through
  `graph.*` operations; node positions become saved fields. Not the
  graph-as-document model. Phases G-1 to G-6 in the decision's table.
  **Next: G-1** the graph IR crate (`docs/briefs/G-1-graph-ir.md`,
  status ready; pure crate, no ECS, no format change). G-1/G-2 do not
  touch the files F1–F3 touch, so the 3D-0 plan can interleave.
  **Before that:** **J-2** cut / selection / boolean stress test is done (merged
  #293; brief `docs/briefs/J-2-cut-select-stress.md`): 7 scenes in
  `tests/it/cut_select_stress.rs`; four fixes: a 3 cm cut no longer
  falls between the contour rows of a long body (the cut contours in
  the stroke frame, `polygonize_components_on_axes`), a stroke through
  one side of a ring opens it, a move no longer records the hold's
  `Kinematic` in its undo step (`HeldKind`), and the newest body of a
  depth band is on top for both drawing and clicks (`StableId::new` is
  UUID v7, `tools::bodies_at_sorted`, `render::stack`); three decisions
  in `archive.ron`.
  **P-1 (perf, 2026-11-20)** max concurrent objects at 60 fps is done
  (merged #300; brief `docs/briefs/P-1-concurrent-objects.md`): avian
  islands (sleeping) re-enabled with the wake seams avian does not own
  (collider/layer regen, joint changes, thrusters, fluids,
  gravity-drive unbind), an enabled
  motor parks `SleepingDisabled` (Box2D's rule), static-static joints
  derive `JointDisabled`, and `physics::island_sync` reconciles island
  links while paused (avian learns joint graph changes through messages
  its paused schedule never reads — bevy expires them after two frames;
  that was the resume panic). Per-frame O(N) costs: `ShapeContours`
  cached beside the collider (borders/snap/selection/debug read it, no
  per-frame re-contour), the snap collider scan skips while paused with
  unchanged inputs, `sync_ground_planes` filters in place, the debug
  overlay pass skips when its toggles are off. `clamp_fixed_catchup` caps
  fixed-timestep catch-up at `ceil(speed)+3` steps per frame — the
  measured desktop death spiral (a settling 4000-box pile self-sustained
  at ~14 steps/frame, 244-397 ms frames) is bounded. `tests/it/sleeping.rs`
  pins the contract incl. the old avian-0.7 repro. Settled 4000-box
  physics: ~12 ms/step before, ~2-4 ms after (headless, single-thread).
  Phase 6 is complete: **6-1** undo round-trip matrix + scene loader
  mutation tests, **6-2** geometry property tests, **6-3** physics debug
  toggles + schedule dump. **6-3** is merged (#295): avian's
  `PhysicsDebugPlugin`
  behind four `DebugSettings` switches (colliders, contacts, joints, AABBs;
  workstation state, never saved or undone) copied onto `PhysicsGizmos` in
  `GizmoConfigStore` by `physics::debug_gizmos` (plugin only with a
  renderer); velocities stay Gradiance's overlay; the hand-rolled collider,
  AABB and contact overlays are gone; `debug.*` operations
  (`(debug-colliders #t)`) and View > Debug over one table
  (`interaction::debug_ops::DEBUG_TOGGLES`), no keys; `gradiance
  schedule-dump [schedule]` prints each schedule's system order as text
  (Bevy's own introspection, no new crate); four decisions in
  `archive.ron`. Overlay checked on lavapipe (screenshot on the PR thread, not committed).
  **Next:** the orchestrator's phase gate: G-0 one graph model, 3D-0 the
  3D plan, D-2 egui 0.37 once bevy_egui ships it.
  **6-2** is merged (#294): geometry property tests (proptest,
  dev-dependency), two fixes (`UnionField` cull, contour drift), six
  decisions.
  **6-1** is merged (#292; `tests/it/undo_matrix.rs` runs
  every history operation (55) from its example: dump, call, undo, redo,
  each dump exact (workstation config left out), 17 s;
  `tests/it/loader_fuzz.rs` loads every committed scene file and 256
  seeded mutations through `scene.load`: typed error and nothing changed,
  or one stable, undoable `load-scene` step, 3 s. Four fixes: pausing no
  longer takes one more step after the run is recorded; a snapshot
  captures an attached node where it follows its target; `from_ron`
  refuses a shared `StableId` and a NaN/inf number (typed errors); six
  decisions in `archive.ron`). Phase N merged (#282–#289, #291); J-1 merged (#290).
  **N-9** mirror and joint load colour is merged (#291): `body.mirror_h` /
  `body.mirror_v` (`(mirror-h targets)`, `(mirror-v targets)`, Edit ▸
  Mirror horizontally / vertically, no key) lower to one `ScaleIntent`
  with a −1 factor about the bodies' bounds centre (one `mirror` undo
  step); a negative scale now reflects each body's rotation and flips its
  shape across one local axis structurally (`geometry::scale::BodyScale`,
  `mirror_shape`), so boxes stay boxes, trees stay trees and mirroring
  twice gives the scene back (test); every scale now carries joint anchors,
  slider axes, node offsets/axes and world pins with its bodies, and a
  fully mirrored hinge turns its limits and motor over; joint glyphs tint
  from the look's joint ink to its danger ink by the derived `JointLoad`
  (`render::joint_viz`, force / break force or the default 10x-weight
  rating, hot from 90 %; `O(joints)`, allocation-free per frame; never
  saved); world pins keep an outer danger ring; five decisions in
  `archive.ron`. **Phase N is complete.** **Next:** the orchestrator plans
  the next stage: Phase 6 (testing & diagnostics), then G-0 one graph
  model, 3D-0 the 3D plan, D-2 egui 0.37 once bevy_egui ships it.
  **J-1** joint stress test is done (merged #290; brief
  `docs/briefs/J-1-joint-stress.md`):
  19 scenes in `tests/it/joint_stress.rs`; five fixes: a body-to-body
  slider no longer explodes at long travel (`track_slider_anchors`), a
  rotation lock stops the spin (`stop_locked_spin`), a hinge on a snapped
  edge joins both bodies, a spring from empty space pins its start, a
  hinge on a bare ground makes nothing; six decisions in `archive.ron`.
  **N-8** laser node is done (in review; branch
  `claude/project-thread-tw1mvl-n8`): `NodeKind::Laser` (bus name, target
  body, axis in the body's frame, range 50 m, 8 bounces, cap 32) casts its
  beam through avian's `SpatialQuery` (`physics::queries::cast_beam`) and
  reflects off every solid body (pure `geometry::beam`);
  `physics::laser::trace_lasers`, one allocation-free `FixedPostUpdate`
  system after the step, `O(lasers × (bounces + 1))` ray casts per step,
  writes the derived `LaserBeam`; `signal::publish_lasers` puts `<name>`
  (1 while the beam reaches the target) and `<name>.distance` on the bus,
  so a light gate drives an N-1 actuator; Laser tool (no key: press, drag
  to aim, release on the target); `(laser body x y angle target name)`
  (`node.laser`); a 45-degree beam off a mirror wall reaches the target
  geometry put on the reflected ray (test); six decisions in
  `archive.ron`.
  **N-7** buoyancy is done (in review; branch
  `claude/project-thread-tw1mvl-n7`): an authored `domain::fluid::Fluid`
  (areal density, default 2 kg/m², and drag rate, default 2/s) on any
  body; every dynamic body overlapping it is pushed up by `ρgA` at the
  submerged centroid and damped (`physics::buoyancy::apply_buoyancy`, one
  allocation-free `FixedUpdate` system); the submerged area is an exact
  clip of the body's derived outline against the fluid's derived convex
  pieces (`geometry::submerge`); a half-plane fluid is an open sea; the
  inspector toggle and `(set-fluid i density [drag])` (`body.set_fluid`)
  also make the body a static sensor in the same undo step; fluids draw
  translucent; a half-density box floats half submerged (measured within
  0.1 %); eight decisions in `archive.ron`.
  **N-6** sketch tool is done (in review; branch
  `claude/project-thread-tw1mvl-n6`): drag freehand; release makes one
  body: the stroke simplified (RDP) into at most 24 capsules joined as one
  balanced SDF union (a straight stroke is one capsule), or a filled
  polygon when it ends near its start and encloses an area; one
  `spawn-body` step through the new `body.sketch` operation
  (`(sketch path width)`; Adjust Last changes the width); stroke width in
  Settings > Grid & Snap > Tools (10 cm); the drag keeps at most 512
  points; Sketch palette entry in Create, no key. `polygonize` samples a
  tree with a thin capsule finer, within the existing sample count
  (`field_cell`), and evaluates unions faster (`sdf::UnionField`, exact);
  six decisions in `archive.ron`.
  **N-5** chain tool is done (in review; branch
  `claude/project-thread-tw1mvl-n5`): drag a path, release lays capsule
  links (link length, width and end pins in Settings > Grid & Snap >
  Tools; pins free / to bodies / to bodies or the world) hinged end to end,
  one `spawn-chain` undo step through the new `body.chain` operation
  (`(chain path link width a b pins)`; Adjust Last works on a drawn chain);
  `SpawnRodCommand` generalised to many bodies; Chain palette entry in
  Connect, no key; a chain pinned at both ends hangs within 2.5 % of the
  catenary sag (test); six decisions in `archive.ron`.
  **N-4** air drag and wind is done (in review; branch
  `claude/project-thread-tw1mvl-n4`): `SimSettings` gains `air_density`,
  `drag_coefficient`, `linear_drag` and `wind` (scene content; drag off by
  default, so old scenes behave the same); quadratic and linear drag scale
  with the body's projected width across the relative air velocity, read
  from a derived 64-sample `AirProfile` built with the collider
  (`geometry::profile`); `physics::air::apply_air_drag`, one allocation-free
  `FixedUpdate` system; Settings > Simulation > Air; `sim-set` paths; five
  decisions in `archive.ron`.
  **N-3** thruster node is done (in review; branch
  `claude/project-thread-tw1mvl-n3`): `NodeKind::Thruster` (thrust in N, an
  axis in the body's frame) pushes its body at its point
  (`physics::thrust::apply_thrusters`, one allocation-free `FixedUpdate`
  system, once per step); the Thruster tool (no key; click pushes up, a drag
  aims); a flame glyph scaled by thrust; the `Thrust` actuator sink with a
  thruster block on the node canvas; `(thruster body x y angle thrust)`
  (`node.thruster`); duplicate copies a thruster's drive; node glyphs are
  screen-sized now; six decisions in `archive.ron`.
  **N-2** graph input keys is done (in review; branch
  `claude/project-thread-tw1mvl-n2`): a node-graph **Key** block
  (`BlockOp::Key`, scene content via `ComputedSignals`) reads 1 (or
  -1/0/1 for a pair) while its key is held in play, published as derived
  `key@<name>` bus inputs by `interaction::graph_keys`; paused or with a
  field focused it reads 0 and the editor keeps every key; in play the
  editor's bare-key uses of a bound key yield (no editor binding changed);
  a capture button and clash warnings in the block footer; `(defkey …)`
  and `(ui-key key [hold])`; new edge interaction → signal; six decisions
  in `archive.ron`.
  **N-1** signal actuators is done (in review; branch
  `claude/project-thread-tw1mvl-n1`): bindings drive a hinge/slider motor's
  speed and cap, a body's field strength and gravity scale through the
  derived `physics::drive::SignalDrive` (never saved, never undone;
  unbinding restores the authored value); one allocation-free actuator
  system; actuator pins and a minimal joint block on the node canvas; six
  decisions in `archive.ron`.
  **Phase R is complete.**
  **R-4** packing as an operation is done (#281 merged): `pack.run` (`(pack targets)`; the
  Optimizer panel invokes the same operation with the selection),
  `pack.status`, `pack.apply`, `pack.cancel`, `pack.get`, `pack.set`; a run
  packs in id order, so a script run and a panel run commit one layout
  (test); R-1's stray root `swing.ron` / `swing.csv` removed and the doc
  example test writes into a temp directory; four decisions in
  `archive.ron`.
  **R-3** insert + clipboard is done (#280): `scene.insert` (File ▸ Insert…),
  `edit.copy` / `edit.paste` (Ctrl+C / Ctrl+V on the canvas, Edit ▸
  Copy / Paste) over scene RON text, one `insert` undo step each, fresh
  ids for everything, nested groups and their names kept; four decisions
  in `archive.ron`.
  **R-2** headless rollout is done (`gradiance run scene.ron [--script
  s.scm]… --steps N [--csv out.csv] [--signals a,b]`, no window, the same
  CSV byte for byte on a re-run). **R-1** run & read surface is done (#278 merged): nine
  `sim.*`/`scene.*`/`signal.*` operations.
  **A-3** second screenshot audit is done (#277: six fixes with tests:
  cuts through jointed pairs, Ctrl chords over a hovered field,
  click-through after any tool commit, the tracer on UI clicks, a tall
  bottom dock, context-menu Ungroup; findings and images in the brief).
  **U-11** Adjust Last Operation is done (#276, stacked on U-10: `LastOp`
  from an operation journal, `edit.adjust_last` / `edit.last_op`,
  `body.array` for the array gesture, the bottom-left panel; four
  decisions in `archive.ron`).
  **U-10** editor areas + saved workspaces is done (#275, stacked on 4b-6:
  presets Layout/Scripting/Signals, a workspace tab row in the top bar,
  `layout.*` operations and `panel.swap`, a right-dock editor-type menu;
  workspaces save with the scene as workstation config; four decisions in
  `archive.ron`).
  **4b-6** toon ramp is done (#274, stacked on U-9: a colour ramp shared by
  every body with a gradient picker in Settings > Rendering > Ramp, matte,
  dithered band edges, front faces rounded from the outline; review
  question `toon-ramp` open, `toon-highlights` withdrawn).
  **U-9** F3 search, status bar and tools pie is done (#272, stacked on
  5-5; tool operations `tool.*`, `ui.search`; review question
  `status-bar-and-pie` open).
  **5-5** frames and tool coordinates decision is done (#271, docs only,
  stacked on 4b-5: a `PlaneFrame` value type at the 2D↔3D lift, phases
  F1–F5 for the next planning stage; recorded as `frames-option` and
  `curvilinear-edges` in `archive.ron`). **4b-5** toon bands on the bodies is done (#270, stacked
  on U-8b; review question `toon-highlights` open). **U-8b** middle-click reset
  is done (#268, stacked on 5-4, #266). The stack #257–#264 merged
  on 2026-11-19 (main 3f211ab): 4b-3 scene looks, U-7 chrome tokens, 4b-4
  scaled defaults (format v8), A-1 screenshot audit (6 fixes), A-2 audit
  follow-ups, 5-2 quantity catalog, 5-3 unit input, U-8 number fields.
  Each brief's Outcome holds its decisions and cuts. The six review
  answers of 2026-11-19 are in `archive.ron`. Open review question:
  `toon-highlights` (4b-5).
  Agent containers need `mesa-vulkan-drivers libvulkan1` for the snapshot
  tests (without them the tests skip unless `GRADIANCE_REQUIRE_GPU=1`).
  Workers run on Opus while Fable lacks usage credits (owner, 2026-11-17).
  The session permission check refuses thread merges: the owner merges.
- **Next, in order** (owner left the order to the planner, 2026-11-17; the
  reasons are in `docs/unified-surface-plan.md` § 10):
  1. **D-1** Bevy 0.20 stack (done, #242; shaders are WESL now).
  2. **U-1** `gradiance-op`: one declaration per operation (done, #243;
     the Outcome lists the migration list).
  3. **U-2a** (done, #244), **U-2b** (done, #245), then **U-2c**
     signal-graph undo (done, #247). (U-2 split 2026-11-17: one PR for all of
     U-2 exceeds one worker's coherence budget.)
  4. **4b-1** group op surface, on the registry (done, #248; adds
     `set-color`, which fixes review scenes re-rolling colours).
  5. **U-3** widget registry + `ui.inspect` + `ui.screenshot` (done,
     #249); **U-4** in-app input injection (done, #250); **U-5**
     snapshot tests on lavapipe (done, #251). **U-3b** `panel.*` operations (done, #253).
  6. **4b-2** groups panel (done, #254).
  7. **U-6** CLI + MCP server (done, #256).
  8. **4b-3** art direction (done, #257), **U-7** theme tokens (done,
     #258), **4b-4** scaled defaults (done, #259); audit **A-1** (done,
     #260) and **A-2** (done, #261); second audit **A-3** (done, #277).
  9. **Phase 5:** 5-2 (done, #262), 5-3 (done, #263), **U-8** number-field
     behaviours (done, #264), then **5-4** (done, #266), **U-8b** middle
     click resets every number field (done, #268), **4b-5** toon bands on the bodies
     (done, #270; both from owner notes, 2026-11-19), then **5-5** (done,
     #271; `docs/frames-decision.md`, `docs/curvilinear-tools-decision.md`).
  9b. **R-1** (done, #278), **R-2** (done), **R-3** (done, #280), **R-4** (done, in review) run & read surface (owner-ranked 2026-11-19; roadmap
      § "Run & read surface", briefs `docs/briefs/R-*`): run/step/save/
      load/dump/export operations, the headless `gradiance run`,
      insert + clipboard, packing as an operation.
  9c. **N-1** (done, in review), **N-2** (done, in review), **N-3** (done,
      in review), **N-4** (done, in review), **N-5** (done, in review), **N-6** (done, in review), **N-7** (done, in review), **N-8** (done, in review), **N-9** (done, in review; Phase N complete) nonlinear dynamics & multibody play (owner-approved
      2026-11-19; roadmap § "Nonlinear dynamics & multibody play",
      briefs `docs/briefs/N-*`): signal actuators, graph input keys,
      thruster, drag + wind, chain, sketch, buoyancy, laser, mirror +
      joint load colour.
  9d. **4b-6** toon ramp (done, #274): themed gradient ramp + picker, matte,
      dithered band edges, normals from the shape (owner direction with a
      reference image, 2026-11-19; supersedes `toon-highlights`).
  10. **U-9** F3 search, status bar, pie menus (done, #272); **U-10**
      editor areas + saved workspaces (done, #275); **U-11** Adjust
      Last Operation (done, #276).
  11. **Phase 6** (testing & diagnostics): **6-1**, **6-2**, **6-3**
      (`docs/briefs/6-*`).
  12. Next planning stage: **G-0** one graph model (plan written,
      2026-11-20: `docs/graph-model-decision.md`; phases **G-1** graph IR
      crate (brief ready), **G-2** graph as scene content, **G-3**
      bindings as edges, **G-4** parametric shapes, **G-5** Lisp view,
      **G-6** Graphite review), **3D-0** the 3D plan, **D-2** egui 0.37
      once bevy_egui ships it.
- **Owner rulings in force:**
  - **Scoped work** (2026-11-17; revises "no unbriefed work" of 2026-11-13).
    The intent is to keep changes local and to keep context from degrading.
    Code changes go through a brief: one brief, one worker, one PR, nothing
    outside its scope. A session may write the brief itself when the work is
    in the plan above. Docs-only changes (plans, briefs, `state.md`) need no
    brief.
  - **Questions go to review mode, not chat** (2026-11-16): add them to
    `docs/review/questions.ron` with scenes; ingest `answers.ron` at each
    phase start. **Only interactive questions go to the owner**
    (2026-11-17: "Leave non interactive changes to the thread here. Go with
    your best estimates"): a plan decision is made by the session on its best
    estimate and recorded in `docs/review/archive.ron` as a `decision: true`
    entry (brief 4b-0b). Six were decided that way on 2026-11-17.
  - **Review answers, ingested 2026-11-17** (`docs/review/archive.ron`):
    the start-up scene stays **empty**; **toon** stays the default shading;
    the fillet bulge is **accepted**.
  - **Review answers, ingested 2026-11-19:** every built default stays:
    **Clay studio** look, the new key/fill lights, **Blender dark with the
    scene accent** chrome, **degrees and metres**, the **settling** spring,
    number fields **as built**. Notes became briefs U-8b (middle-click
    reset) and 4b-5 (bands on the bodies, like specular highlights); a
    real slider widget is retained (UI/render).
  - **Format and UI may change freely** (2026-11-16): bump `FORMAT_VERSION`,
    reject older files, regenerate goldens, no migrations.
  - **Merge freely when CI is green** and no tests or checks were removed to
    get there (2026-11-16).
  - **Orchestrator rotation at gates is skipped.**
  - **Stacked PRs (2026-11-18):** threads keep working as stacked PRs, one
    branch per brief on the previous; the owner merges bottom-up.
  - **UI and op-surface direction** (2026-11-17, `docs/unified-surface-plan.md`
    § 0): excellent egui polish is now wanted (overrides `orchestration.md`
    Future intent 1's "don't polish"); lean to Blender's theme; upgrade crates
    where it saves rework; Graphite is the capability goal, MVP first; port
    from upstreams, don't depend; BUI dropped; licensing not a concern yet;
    UI/Lisp/CLI/MCP/graph must be one surface that cannot drift.
  - **Screenshots may go in git when useful** (2026-11-17), e.g. snapshot
    test fixtures. Owner review of visuals goes through review mode (4b-0),
    not ad-hoc images.
  - **Keys are out of scope** (2026-11-17): keep the current bindings; no
    keymap or Blender-key work.
  - **Earlier architecture decisions may be revisited** where the intent has
    moved (2026-11-17). First use: dock layouts and workspaces persist as
    workstation config (U-10).
- **Process:** one worker at a time (shared disk). The worker hands off via
  the brief's Outcome + PR. The orchestrator reviews, merges (REST squash),
  updates this file, and sends screenshots to the owner.
- **Screenshots:** `tools/screenshot/` (lavapipe + Xvfb). Ad-hoc ones stay
  out of git; they are shown in the conversation and described in PR
  comments. Images that a test or a review question uses may be committed
  (ruling above).

## Retained for future planning — do not work on these

Kept for the next planning stage. Workers must not pick these up, and briefs
must not absorb them. A **bug found by a screenshot or test** in a landed
feature is different: it gets a brief, as 2-5 and 4-4 did.

- **Physics/constraints:**
  - backlash, cams, planar contact (undefined);
  - torque-based breaking (2-2);
  - rod fixed end = merge, `spawn-rod`, rod inspector (2-3);
  - joint node-graph block (2-1): a minimal one landed with N-1 (sensor
    outputs, motor inputs); editing, "add to node editor" and layout remain;
  - `signal.bind` / `signal.unbind` operations (N-1 Outcome);
  - short gravity-loaded arms lose motor speed (0.1 m arm ~27% slowest);
    substeps and the ceiling are not the cause (A-2 Outcome, proposal);
  - field strength does not scale with the source size (fixed 1 m falloff,
    500 m/s² clamp; A-2 Outcome; needs a feel question with scenes);
  - a 2 m flat drop lands 1–4 cm sideways in avian (2-5);
  - unaudited despawned-entity `insert` pattern (2-5);
  - elastica/flexure from #194 (after Phase 5).
- **Geometry:**
  - CSG arcs snap to contour vertices and give no tangents (1-4, 1-7);
  - split boolean pieces become polygon leaves; the AABB-level miss check
    (4-1);
  - smooth-min bulges flush faces (≤ r/4): **accepted** by the owner in
    review (2026-11-17), kept as a low-priority issue; the fillet body's selection box
    ignores the bulge; smooth subtract/intersect, chamfers, corner fillets
    (4-2).
- **Snapping/tools:**
  - alignment during creation tools (1-5);
  - grid coverage from the frustum (1-1);
  - tangent-snap toggle and the tangent glyph bar always horizontal (1-7);
  - faint alignment guide.
- **Scripting:**
  - groups panel (4b-2): dock widths below 260 points and a real
    double-click are unchecked;
  - `bodies-where` has no label filter: labels live in `gradiance-script`,
    above `command`, and need a mirror first (4b-1 Outcome);
  - verbs for `ToolDefaults`/`SnapConfig`/joint props;
  - `joint-at`, `node-at`/node indexing, `set-orbit`, `set-depth` (3-3);
  - script tools:
    - multi-edit gives one undo step per command;
    - no `unregister-tool`;
    - palette scrolling;
    - drag/`ManipTool` tools, previews, icons, shortcuts (3-5);
  - two start-up scripts share one snapshot;
  - pixel-era sizes in `tests/it/scripting.rs` (3-6).
- **Units (5-1):** tracer dot minimum radius 0.5 m, tracer size range
  0.5–20 m and slider-limit 1 m gap are pixel-era; spring damping unit vs
  avian's; geometry crate retype; enum joint kind dumps as `Void`.
- **UI/render:**
  - visuals revamp (owner, review 2026-11-17): the first build-out had no
    screenshots; 2D lighting (e.g. a Bevy 2D-lighting crate) is worth a look;
    the lighting is flat, so the toon bands were never really seen. 4b-3
    takes the part that fits its scope; the rest is for the next planning
    stage;
  - Joint window covers the Properties dock;
  - a real slider widget for number fields (owner note, 2026-11-19);
  - opening a dock re-centres and rescales the view (A-1 finding 13;
    proposal in the A-2 Outcome);
  - report Bevy's SSAO-viewport bug upstream (then drop the sub-view
    workaround); `world_per_pixel` uses window height for the perspective
    FOV (4-4).
  - Headless `ButtonInput` presses lose `just_pressed` (3-5).
- **Review mode (4b-0):** option-level camera (open a question tilted);
  seed joint-inspector / tangent-glyph / guide-faintness questions once an
  alternative exists; option scripts' registered actions persist across
  options.

## Parked — do not merge, rebase or close

- **#208** (draft) avian2d → bevy_rapier3d + simulation planes: input to the
  3D planning stage and to 5-5 (read only).
- **#194** closed and salvaged into 2-3; its branch is kept for the elastica
  spike.
- Stale branches without PRs: left alone; prune at a gate if unclaimed.

## Known hazards

- **Shaders fail at run time, not at build time** (D-1): after a shader
  edit, start the app and search the log for `failed to create shader
  module`.
- **Disk:** one worker at a time. Between workers run
  `rm -rf target/debug/deps/*gradiance* target/debug/incremental/*
  target/debug/gradiance`. If still short, delete
  `find target/debug/deps -atime +2`. Never `cargo clean`. Run
  `cargo test --workspace` locally only above ~20 GB free; otherwise leave
  it to CI.
- **Toolchain:** rustc 1.100.0 = `rust-toolchain.toml` = `RUST_PIN` (D-0).
  `rustup` installs it on the first `cargo` call (21 s, 2026-11-17). The
  container default is still 1.97.0: run `cargo` from the repo root.
  A fresh container needs `apt-get update` before the native-library install.
  The screenshot run also needs `libxkbcommon-x11-0`.
- **Long runs go to CI** (owner, 2026-11-17): do not run
  `cargo test --workspace` in a container; push and read the checks.
- **CI:** read annotations first (fmt, clippy and test failures are
  annotated); full logs via `gh run view <id> --log-failed`. GraphQL 403s:
  merge with `gh api -X PUT …/pulls/N/merge -f merge_method=squash -f
  sha=…`. Ready-for-review: `POST …/pulls/N/ccr/ready_for_review`.
- **Shallow clone:** after pushing a branch, add a fetch refspec for it.
- **Screenshot harness** (`tools/screenshot/`, driven by operations since
  U-4):
  - the sim plays from start-up: `ui.key "Space"` pauses; undo does nothing
    while it plays;
  - world (0,0) is window (640,370) at 100 points/m; prefer widget names
    from `ui.inspect`;
  - a gesture plays over the next frames: wait before the next grab;
  - indexed verbs in one run read the pre-run snapshot, so apply them from a
    second, hot-reloaded script.
- Where `docs/agent-context.md` and `CLAUDE.md` disagree, `CLAUDE.md` wins.
