# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-25, Phase 1 gate passed → Phase 2 (orchestrator).

## Position

- **Phase:** 2 — Constraints & substrate completion
  (`docs/orchestration.md` § The plan).
- **main:** `73e1485`. **Phase 1 (M18) done:** #212 adaptive snap, #214
  major/minor lines, #213 axis lock follows the grid, #215 analytic circle
  snap features, #216 alignment while moving, #217 three-point box, #218
  tangent snap + tangent circle. Curvilinear coordinates moved to a Phase 5
  decision note, as planned. Every PR's CI was green at merge (CI runs on
  PRs only; main has no push CI).
- **Orchestrator rotation skipped at this gate** on the owner's instruction
  ("continue as far as you can tonight"). The files are complete enough for
  a fresh orchestrator to take over at any point.
- **Phase 2 briefs, run serially:**
  - **2-1** joint sensor sources — `docs/briefs/2-1-joint-sensors.md`.
  - **2-2** breaking limits — `docs/briefs/2-2-breaking-limits.md` (uses
    avian 0.7 `JointForces` + `JointDisabled`; "broken" is authored state).
  - **2-3** rigid rod, salvaged from #194 — `docs/briefs/2-3-rigid-rod.md`.
  - **2-4** required components on `Body`/`Joint` —
    `docs/briefs/2-4-required-components.md`.
  - Not yet briefed: backlash; cams / planar contact.
  - Already landed (found at the gate): joint config in the context menu.
- **Workers:** hand off through the brief's Outcome and the PR; the
  orchestrator updates this file after review and merges.
- **Open follow-ups (unbriefed):** CSG shapes containing arcs still snap to
  contour vertices and give no tangents (1-4, 1-7 Outcomes); alignment during
  creation tools (1-5 Outcome); grid coverage from the frustum (1-1); no
  script verb for `ToolDefaults`/`SnapConfig` — only `sim-set` (1-6; Phase
  3); no on/off toggle for tangent snapping (1-7).

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
