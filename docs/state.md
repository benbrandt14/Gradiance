# State — where the project is right now

Overwritten at every handoff; history lives in git and in each brief's
Outcome (`docs/briefs/`). Keep it to about a screen.
Last updated: 2026-11-17 (U-2c worker handoff, PR in review; phase gate
after U-2: U-1 #243, U-2a #244, U-2b #245; order per
`docs/unified-surface-plan.md` § 10 and § 13).

## Position

- **Done:** Phases 0–4 (M18 grids/snap, constraints, scripting & op surface,
  CSG booleans/fillet, M19 render + chrome). Phase 2's backlash, cams and
  planar contact are still undefined (retained below). PRs #212–#235. Each
  brief's Outcome records its decisions; read it before touching that area.
- **Now:** **U-2c in review** (PR #247, branch
  `claude/project-thread-dydbi0`): signal-graph edits record one settled
  `settings` undo step; the scene-content list is one macro in
  `scene::records`. Orchestrator: read the brief's Outcome, merge when CI is
  green. The U-2 phase is done: every Lisp verb is a
  `gradiance-op` operation (51) or one of 11 listed VM built-ins; menus and
  keys invoke operations by id; `docs/ops.md` is generated; a scanner bounds
  intent writers in `gradiance-ui`/`-interaction`. **Next: 4b-1** (brief revised
  for the registry). Workers run on Opus while Fable lacks usage credits
  (owner, 2026-11-17).
- **4b-1 was not in flight:** GitHub had no branch and no PR for it on
  2026-11-17. It moved to step 4; its verbs are written once, on the
  registry (`docs/review/archive.ron`, `order-4b-1`).
- **Next, in order** (owner left the order to the planner, 2026-11-17; the
  reasons are in `docs/unified-surface-plan.md` § 10):
  1. **D-1** Bevy 0.20 stack (done, #242; shaders are WESL now).
  2. **U-1** `gradiance-op`: one declaration per operation (done, #243;
     the Outcome lists the migration list).
  3. **U-2a** (done, #244), **U-2b** (done, #245), then **U-2c**
     signal-graph undo (in review, #247). (U-2 split 2026-11-17: one PR for all of
     U-2 exceeds one worker's coherence budget.)
  4. **4b-1** group op surface, on the registry (adds `set-color`, which
     fixes review scenes re-rolling colours).
  5. **U-3** widget registry + `ui.inspect` + `ui.screenshot`;
     **U-4** in-app input injection; **U-5** snapshot tests on lavapipe.
  6. **4b-2** groups panel (on the new registry).
  7. **U-6** CLI + MCP server.
  8. **U-7** theme tokens, Blender-leaning, together with **4b-3** art
     direction; then **4b-4** scaled defaults.
  9. **Phase 5:** 5-2, 5-3, then **U-8** number-field behaviours, 5-4, 5-5.
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
  - world-pin angle sign, joint node-graph block (2-1);
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
- **Screenshot harness:**
  - keyboard input doesn't reach the app;
  - egui buttons: `mousedown 1; sleep 0.3; mouseup 1` (egui drops clicks
    held > 0.8 s); scene picks want a ~1 s hold;
  - pause the sim at screen (60,42);
  - world (0,0) is screen (640,370) at 100 px/m;
  - open panels with `(panel-show …)`;
  - indexed verbs in one run read the pre-run snapshot, so apply them from a
    second, hot-reloaded script.
- Where `docs/agent-context.md` and `CLAUDE.md` disagree, `CLAUDE.md` wins.
