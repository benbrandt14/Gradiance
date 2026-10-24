# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-24, Phase 0 (orchestrator).

## Position

- **Phase:** 0 — Reset (`docs/orchestration.md` § The plan).
- **Next brief:** `docs/briefs/1-1-adaptive-snap.md` (status: ready).
- **main:** `b1418a8` — PR #210 (action layer) merged 2026-10-24.
  CI on main: **red at Clippy** on the newer `@stable` toolchain; the code was
  last green on 1.97 (July). Fix: PR #211.
- **Open PR #211** `claude/resurrect-orchestration` (draft): CI annotations,
  the toolchain clippy fix, this file, `orchestration.md`, the first brief,
  `frames-decision.md` stub.

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

- Local toolchain in the agent container: rustc 1.97.0. CI `@stable` is newer.
- Last full suite (July, per the previous session): 233 integration tests,
  62 in `gradiance-ui`, 0 failures. Not re-run this session.

## Known hazards

- Container can't reach rust-lang.org (no toolchain update) and can't read
  Actions logs — read CI through annotations.
- Where `docs/agent-context.md` and `CLAUDE.md` disagree, `CLAUDE.md` wins.
