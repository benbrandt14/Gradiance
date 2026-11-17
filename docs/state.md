# State — where the project is right now

Overwritten at every handoff; history lives in git and in each brief's
Outcome (`docs/briefs/`). Keep it to about a screen.
Last updated: 2026-11-16 (orchestrator; #238 merged; 4b-1 in flight).

## Position

- **Done:** Phases 0–4 (M18 grids/snap, constraints, scripting & op surface,
  CSG booleans/fillet, M19 render + chrome). Phase 2's backlash, cams and
  planar contact are still undefined (retained below). PRs #212–#235. Each
  brief's Outcome records its decisions; read it before touching that area.
- **Now:** **4b-1** group op surface. Recently merged: **5-1** (#236,
  typed quantities, format v7) and **4b-0** (#238, review mode:
  `cargo run -- --review`, questions/answers in `docs/review/`, process in
  `orchestration.md` § Review mode; 3 seed questions open).
- **Next, in order:**
  - **Phase 4b** (owner direction 2026-11-16, `orchestration.md`):
    - **4b-1** group op surface (in flight; adds `set-color`, which fixes
      review scenes re-rolling colours);
    - **4b-2** groups panel;
    - **4b-3** art direction (2–3 presets, SSAO options; show the owner);
    - **4b-4** parameters scaled to body mass, with behaviour tests.
  - **Phase 5:**
    - **5-2** named quantity catalog (P3);
    - **5-3** unit-aware input + deg/rad preference (P4);
    - **5-4** units in scripts (P6);
    - **5-5** frames + curvilinear proposals (owner ratifies).
  - Then **Phase 6** (testing & diagnostics). No briefs yet.
- **Owner rulings in force:**
  - **No unbriefed work** (2026-11-13).
  - **Questions go to review mode, not chat** (2026-11-16): add them to
    `docs/review/questions.ron` with scenes; ingest `answers.ron` at each
    phase start.
  - **Format and UI may change freely** (2026-11-16): bump `FORMAT_VERSION`,
    reject older files, regenerate goldens, no migrations.
  - **Merge freely when CI is green** and no tests or checks were removed to
    get there (2026-11-16).
  - **Orchestrator rotation at gates is skipped.**
- **Process:** one worker at a time (shared disk). The worker hands off via
  the brief's Outcome + PR. The orchestrator reviews, merges (REST squash),
  updates this file, and sends screenshots to the owner.
- **Screenshots:** `tools/screenshot/` (lavapipe + Xvfb). They never go in
  git; they are shown in the conversation and described in PR comments.

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
  - smooth-min bulges flush faces (≤ r/4); the fillet body's selection box
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
  - start-up scene has no ground (product decision, 3-6);
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

- **Disk:** one worker at a time. Between workers run
  `rm -rf target/debug/deps/*gradiance* target/debug/incremental/*
  target/debug/gradiance`. If still short, delete
  `find target/debug/deps -atime +2`. Never `cargo clean`. Run
  `cargo test --workspace` locally only above ~20 GB free; otherwise leave
  it to CI.
- **Toolchain:** rustc 1.97.0 = `RUST_PIN`; rust-lang.org is unreachable.
  Pending bump debt: `orchestration.md` § Recurring.
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
