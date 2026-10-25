# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-24, Phase 1 (orchestrator, after 1-1 merged).

## Position

- **Phase:** 1 — M18 grids & snapping (`docs/orchestration.md` § The plan).
- **main:** `5bb18b7` — PR #212 (brief 1-1, adaptive snap) merged. Both
  orchestrator calls in its Outcome accepted: the grid draws under
  perspective, and isometric snaps to drawn-line intersections.
- **In flight — three workers in parallel** (file-disjoint by design):
  - **1-2** major/minor lines — `docs/briefs/1-2-major-minor-lines.md`
    (render `grid.rs` + geometry `snapping.rs`).
  - **1-3** axis lock follows the grid — `docs/briefs/1-3-axis-lock-basis.md`
    (interaction `gesture.rs`, `indicators.rs` guides, `tools/select.rs`).
  - **1-4** steady snap glyphs + snap-off pin —
    `docs/briefs/1-4-snap-glyph-stability.md` (interaction `snap.rs`,
    geometry snap features).
  All three may append to `geometry/snapping.rs`; expect trivial rebases.
- **Parallel-run rule:** workers do **not** edit this file (they would
  conflict). Each hands off through its brief's Outcome and its PR; the
  orchestrator folds results in here after review.
- **Next briefs (not written):** 1-5 collinear/centreline snapping (after
  1-4 lands — same snap engine), 1-6 alternate constructors (3-point box,
  tangent circle; also owns the tangent snap kind/glyph deferred from 1-4).

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
