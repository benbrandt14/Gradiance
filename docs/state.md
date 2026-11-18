# State — where the project is right now

Overwritten at every handoff; history lives in git and in each brief's
Outcome (`docs/briefs/`). Keep it to about a screen.
Last updated: 2026-11-18 (5-2 handoff, stacked on A-2; A-2 handoff, stacked on A-1; A-1 handoff, stacked on 4b-4; 4b-4 handoff, stacked on U-7; U-7 handoff, stacked on 4b-3 #257; phase gate
after U-6, #256, b53b019; order per `docs/unified-surface-plan.md` § 10
and § 13).

## Position

- **Done:** Phases 0–4 (M18 grids/snap, constraints, scripting & op surface,
  CSG booleans/fillet, M19 render + chrome). Phase 2's backlash, cams and
  planar contact are still undefined (retained below). PRs #212–#235. Each
  brief's Outcome records its decisions; read it before touching that area.
- **Now:** **4b-3** art direction in review (PR #257): three scene looks
  (`gradiance_domain::theme`, studio default), one colour source
  (boundaries test), toon bands now visible (cap winding, light levels,
  banded planes), review questions `look` and `toon-lighting` added.
  **U-7** theme tokens in review, **stacked on 4b-3** (branch
  `claude/project-thread-tw1mvl-u7`, base #257's branch; merge #257
  first): `ui::theme` chrome tokens, Blender-dark default, the accent and
  selection from the scene look, presets (`chrome.apply`, workstation
  config, not saved), the colour scanner now covers `gradiance-ui`,
  review question `chrome`. **4b-4** scaled defaults in review, **stacked
  on U-7** (branch `claude/project-thread-tw1mvl-4b4`; merge #257, then
  #258, then it): springs authored as frequency + damping ratio (format v8),
  the motor gain fix (the default motor did not turn), clamped restitution
  and friction, weight-scaled break force, behaviour tests
  (`tests/it/defaults.rs`), review question `spring-feel`. **A-1**
  screenshot audit in review, **stacked on 4b-4** (branch
  `claude/project-thread-tw1mvl-a1`; merge #257, #258, #259, then it):
  six fixes (drag tools started one step late; world-pin hinge reversed;
  motor gain ignored the sim speed; Groups chips selected whole groups;
  the `spring` verb anchored b at a's centre, which also bent the
  `spring-feel` scenes; the palette covered the bottom dock), two findings
  for new briefs (world-pin slider reversed, overlapping groups); images
  in `/mnt/project-files/a-1/`. **A-2** audit follow-ups in review (PR
  #261, branch `claude/project-thread-tw1mvl-a2`, **stacked on A-1** #260
  → #259 → #258 → #257; merge those first): world-pin sliders derived
  pin-first like hinges; grouping a grouped body brings its whole group and
  a subset nests inside (archive `group-overlap`); the motor inspector reads
  20 rpm and shows the derived auto torque. Recorded with proposals, not
  fixed: dock opening re-centres the view, short arms lose motor speed
  (not substeps, not the ceiling), field strength scaling (needs a feel
  question). **5-2** quantity catalog in review, **stacked on A-2**
  (branch `claude/project-thread-tw1mvl-5-2`; merge #261 → #260 → #259 →
  #258 → #257 first): one named table (`domain::quantity`, 14 rows) that
  the signal sources, sensor ports, node-graph pins, signals panel and plot
  units read; `quantity.*` operations (`(quantities)`, `(quantity name i)`,
  `(quantity-between name a b)`, `(quantity-unit name)`). **U-6** #256 is merged (b53b019): `gradiance ops`,
  `gradiance op`, `gradiance mcp [--launch]` are clients of a `--remote`
  app; the catalog carries a params schema and an example per operation.
  Agent containers need `mesa-vulkan-drivers libvulkan1` for the snapshot
  tests (without them the tests skip unless `GRADIANCE_REQUIRE_GPU=1`).
  Workers run on Opus while Fable lacks usage credits (owner, 2026-11-17).
  The session permission check refuses thread merges: the owner merges
  (#253, #254, #256, 2026-11-18).
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
  8. **4b-3** art direction (scene look; in review, #257), then **U-7** theme
     tokens, Blender-leaning (chrome; in review, stacked on #257); then
     **4b-4** scaled defaults (in review, stacked on U-7).
  9. **Phase 5:** 5-2 (in review), 5-3, then **U-8** number-field behaviours, 5-4, 5-5.
  10. **U-9** F3 search, status bar, pie menus; **U-10** editor areas +
      saved workspaces; **U-11** Adjust Last Operation.
  11. **Phase 6** (testing & diagnostics).
  12. Next planning stage: **G-0** one graph model, **3D-0** the 3D plan,
      **D-2** egui 0.37 once bevy_egui ships it.
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
  - joint node-graph block (2-1);
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
