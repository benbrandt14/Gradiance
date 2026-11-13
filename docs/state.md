# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-11-12, Phase 2 done → Phase 3 next (orchestrator).

## Position

- **Phase:** 2 complete except three undefined items (below) → **Phase 3**
  (scripting & the op surface) next. Orchestrator rotation skipped again at
  the owner's instruction ("continue as far as you can").
- **main:** `7614ca3`. Phase 2 merged: #219 joint sensors (2-1), #220
  breaking limits (2-2), #221 rigid rod + Spring rename (2-3), #222 required
  components (2-4), #223 screenshot-pass bugs (2-5: `sim-set` timing,
  scripted-body colour, and the CI flake), #224 CI test-failure annotations.
- **Phase 3 briefs:** **3-1** script step budget —
  `docs/briefs/3-1-script-budget.md` (steel 0.8.2 has `with_interrupted` +
  `InterruptHandler`; in flight). Then **3-2** group/ungroup verbs,
  **3-3** action table (click point + folded context-menu commands),
  **3-4** remote control surface (BRP over the op registry). Still to
  brief: user tools from `.scm` (after 3-3).
- **Screenshots:** `tools/screenshot/` (lavapipe + Xvfb). Images stay out of
  git; show them in the conversation and describe them in PR comments.
- **Workers:** hand off through the brief's Outcome and the PR; the
  orchestrator updates this file after review and merges.
## Retained for future planning — do not work on these

Owner instruction (2026-11-13): **no unbriefed work.** Everything below is
kept for the next planning stage; workers and orchestrators must not pick
it up, and briefs must not absorb it. Defined roadmap items (the phases in
`docs/orchestration.md`) continue.

- **Undefined Phase 2 items:** backlash, cams, planar contact (named in the
  roadmap/feedback only; behaviour unspecified).
- **Follow-ups from briefs' Outcomes:** CSG arcs snap to contour vertices
  and give no tangents (1-4, 1-7); alignment during creation tools (1-5);
  grid coverage from the frustum (1-1); no tangent-snap toggle (1-7); verbs
  for `ToolDefaults`/`SnapConfig` and joint properties (1-6, 2-2); world-pin
  joint-angle sign vs `limit_reference_angle`'s comment, and a joint
  node-graph block (2-1); torque-based breaking (2-2); rod: fixed end =
  merge, `spawn-rod` verb, rod inspector (2-3); a 2 m flat drop lands 1–4 cm
  sideways inside avian (2-5); other systems may share the
  despawned-entity `insert` panic pattern fixed in 2-5 (unaudited).
- **Elastica / flexure** from #194 (after Phase 5).

A **bug found by a screenshot or test** in a landed feature is not
"unbriefed work": it gets a brief (as 2-5 did) because it is the defined
item not yet meeting its brief.

## Parked PRs — do not merge, rebase, or close without the orchestrator

- **#208** (draft, 2026-07-29) avian2d → bevy_rapier3d + simulation-plane
  groundwork. Input to the post-roadmap 3D planning stage.
- **#194** — **closed at the Phase 1 gate**, salvaged into brief 2-3
  (pre-crate-split, reintroduces the removed weld joint, conflicting v6).
  Its elastica/flexure solver is a later spike (after Phase 5 units). The
  branch is kept for reference.

Stale branches with no open PR (`claude/sim-spike`,
`claude/scripting-reflection-unify-*`, `claude/project-architecture-rewrite-*`,
`claude/gradiance-crate-adoption-audit-*`) are left alone; prune at a phase
gate if nobody claims them.

## Last verified

- Agent container rustc 1.97.0 = `RUST_PIN`.
- Last merged PR #218 green in CI (ci, coverage, coupling) on its head.
- Pending toolchain-bump debt: `orchestration.md` § Recurring.

## Known hazards

- **Parallel workers on one container exhaust disk.** Three workers sharing
  one `CARGO_TARGET_DIR` hit 94% (27 GB target, mostly two Bevy feature
  sets: workspace vs `-p`). Shared target also races: one branch's build
  overwrites another's artefacts (a spurious local doc failure on #214).
  Prefer one worker per container, or serialize; leave the workspace test
  gate to CI when `df` is under ~8 GB. Between workers, free space with
  `rm -rf target/debug/deps/*gradiance* target/debug/incremental/*` (keeps
  the third-party build; ~15 GB back).
- Container can't reach rust-lang.org (no toolchain update). CI failures:
  annotations first (clippy, fmt and now test failures are annotated); full
  logs via `gh run view <run-id> --log-failed` (works as of 2026-11-12). Format failures now annotate
  too (file per error).
- A fresh container needs `apt-get update` before the native-lib install.
  The first `cargo test -p <crate>` compiles a second Bevy feature set
  (~20 min); start it early.
- The shallow clone tracks only `main`: after pushing a branch, add a fetch
  refspec for it, or the stop-hook reports "no remote branch".
- `gh pr ready` / `gh pr merge --auto` use GraphQL and 403. The REST
  routes work: `gh api -X POST repos/{o}/{r}/pulls/{n}/ccr/ready_for_review`
  (also `…/ccr/convert_to_draft`, `PUT|DELETE …/ccr/auto_merge`).
- Where `docs/agent-context.md` and `CLAUDE.md` disagree, `CLAUDE.md` wins.
