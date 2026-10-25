# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-24, Phase 1 (worker, brief 1-1).

## Position

- **Phase:** 1 — M18 grids & snapping (`docs/orchestration.md` § The plan).
- **main:** `d6b83f5` — PR #211 (Phase 0 reset) merged; Phase 0 gate passed.
- **PR #212** `claude/1-1-adaptive-snap` (brief 1-1, status: done):
  snapping uses the same adaptive step the grid is drawn at, in every grid
  system (`geometry::snapping::adaptive_step`). Also fixes isometric snap
  targets that sat off the drawn lines, draws the grid under perspective,
  guards a zero spacing, and fixes the CI annotate step (Format failures
  were reported as a missing `clippy.txt`). Local gates all green.
  CI green (ci, coverage, coupling); ready for review. **Awaiting
  orchestrator review**, including two calls flagged in the
  brief's Outcome: grid now drawn under perspective (vs. disabling snap
  there), and the isometric snap-target change.
- **Next brief:** 1-2 major/minor lines — not yet written (orchestrator).
  It reuses `adaptive_step`. The Outcome of 1-1 notes that majors currently
  count adaptive steps, so they shift as the step doubles.

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
- PR #212 locally (2026-10-24): fmt, clippy `-D warnings`, doc
  `-D warnings`, `cargo test --workspace` (534 passed) all green.
- Pending toolchain-bump debt: `orchestration.md` § Recurring.

## Known hazards

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
