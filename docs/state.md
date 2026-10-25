# State — where the project is right now

Overwritten at every handoff; history lives in git. Keep it under a screen.
Last updated: 2026-10-25, Phase 1 (orchestrator, briefs 1-2..1-4 reviewed).

## Position

- **Phase:** 1 — M18 grids & snapping (`docs/orchestration.md` § The plan).
- **main:** `5bb18b7` — PR #212 (brief 1-1, adaptive snap) merged. Both
  orchestrator calls in its Outcome accepted: the grid draws under
  perspective, and isometric snaps to drawn-line intersections.
- **Reviewed, ready, awaiting merge by the owner** (all CI green: ci,
  coverage, coupling; each brief's Outcome filled, status `done`):
  - **#214** brief 1-2 major/minor lines — majors every 4th adaptive step
    (`GRID_MAJOR_EVERY`, power of two ⇒ fixed world positions).
  - **#213** brief 1-3 axis lock follows the visible grid's basis
    (projection onto the lock axis; polar uses the grid's rotated frame).
  - **#215** brief 1-4 steady circle snap glyph (`geometry::snap_features`,
    analytic circle features) + hidden-grid-snaps-nothing test.
  Code merges cleanly pairwise except two append-only spots: `docs/roadmap.md`
  (1-2 vs 1-3) and `tests/it/interaction.rs` (1-3 vs 1-4). Whichever lands
  later rebases; both are keep-both resolutions.
- **Parallel-run rule:** workers do **not** edit this file (they would
  conflict). Each hands off through its brief's Outcome and its PR; the
  orchestrator folds results in here after review.
- **Next briefs (not written):** 1-5 collinear/centreline snapping (after
  #215 merges — same snap engine; also: CSG shapes containing arcs still
  snap to tessellation points, per 1-4 Outcome), 1-6 alternate
  constructors (3-point box, tangent circle; owns the tangent snap
  kind/glyph — 1-4 Outcome has the formula and the reference-point gap).

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
  gate to CI when `df` is under ~8 GB.
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
