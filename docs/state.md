# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-24, Phase 0 (orchestrator).

## Position

- **Phase:** 0 — Reset (`docs/orchestration.md` § The plan).
- **Next brief:** `docs/briefs/1-1-adaptive-snap.md` (status: ready).
- **main:** `b1418a8` — PR #210 (action layer) merged 2026-10-24.
  CI on main: **red at Clippy** — `@stable` had moved past 1.97 and new
  pedantic lints became errors under `-D warnings`. PR #211 fixes it.
- **PR #211** `claude/resurrect-orchestration`: CI pinned to `RUST_PIN`
  1.97.0; clippy failures re-emitted as readable annotations; one geometry
  lint fixed forward (`as_chunks`); `CLAUDE.md` agent-session section; this
  file; `orchestration.md`; brief 1-1; `frames-decision.md` stub.
  **CI fully green** (ci, coverage, coupling). Awaiting merge — merging it
  turns main green and closes Phase 0.

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

- Agent container rustc 1.97.0 = `RUST_PIN`. Local clippy clean.
- PR #211 CI run 40134366700: all gates green on 1.97.0.
- Pending toolchain-bump debt: `orchestration.md` § Recurring.

## Known hazards

- Container can't reach rust-lang.org (no toolchain update) and can't read
  Actions logs — read CI through annotations.
- Where `docs/agent-context.md` and `CLAUDE.md` disagree, `CLAUDE.md` wins.
