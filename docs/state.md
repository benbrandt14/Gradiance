# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-25, Phase 1 (orchestrator, after #216 merged; 1-6 in flight).

## Position

- **Phase:** 1 — M18 grids & snapping (`docs/orchestration.md` § The plan).
- **main:** `ee4fe8a` — briefs 1-1..1-5 merged: #212 adaptive snap, #214
  major/minor lines (majors every 4th step, fixed world positions), #213
  axis lock follows the visible grid's basis, #215 analytic circle snap
  features + hidden-grid-snaps-nothing test, #216 alignment snapping while
  moving (`geometry::align`, oblique grid coordinates; `SnapConfig.align_edges`
  on, `align_centers` off).
- **Next, serial (one worker at a time — see hazards):**
  - **1-6** three-point box — `docs/briefs/1-6-three-point-box.md` (in flight).
  - **1-7** tangent snap + tangent circle — `docs/briefs/1-7-tangent-snap.md`
    (after 1-6, so its multi-click mode publishes the snap reference).
  1-6 and 1-7 split the roadmap's "alternate constructors" bullet.
- **Workers:** hand off through the brief's Outcome and the PR; the
  orchestrator updates this file after review and merges.
- **Open follow-ups (unbriefed):** CSG shapes containing arcs still snap to
  contour vertices (1-4 Outcome: per-leaf features clipped to the CSG
  boundary); alignment during creation tools (1-5 Outcome has the wiring); grid
  coverage sized from the frustum (1-1 Outcome).

## Parked PRs — do not merge, rebase, or close without the orchestrator

- **#208** (draft, 2026-07-29) avian2d → bevy_rapier3d + simulation-plane
  groundwork. Input to the post-roadmap 3D planning stage.
- **#194** (draft, 2026-07-19) rigid rod + elastica flexure spike. Decision
  scheduled in Phase 2 (salvage into briefs, or close with a note).

Stale branches with no open PR (`claude/sim-spike`,
`claude/scripting-reflection-unify-*`, `claude/project-architecture-rewrite-*`,
`claude/gradiance-crate-adoption-audit-*`) are left alone; prune at a phase
gate if nobody claims them.

## Last verified

- Agent container rustc 1.97.0 = `RUST_PIN`.
- main `5bb18b7` green in CI (PR #212: ci, coverage, coupling).
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
- Container can't reach rust-lang.org (no toolchain update) and can't read
  Actions logs — read CI through annotations. Format failures now annotate
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
