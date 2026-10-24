# Orchestration — how this project is driven across many agent sessions

Every agent thread becomes decoherent with enough time: early context gets
compressed or forgotten, assumptions drift, and a long session starts
contradicting decisions it made hours ago. This document is the operating
manual that makes the project survive that. **The rule underneath all of it:
nothing that matters lives only in a conversation.** If the next session
needs it, it is in the repo.

Read order for any fresh session: `CLAUDE.md` (the contract) →
`docs/state.md` (where things are) → this file (how to work) → your brief
in `docs/briefs/` (what to do). That is the whole boot. Do not read the
roadmap or decision records end to end unless your brief tells you to.

## Roles

**Orchestrator.** A planning session. Owns `docs/roadmap.md`,
`docs/state.md`, and `docs/briefs/`. Picks the next item, writes its brief,
reviews the returning PR against the brief, merges, updates state. **Writes
no feature code.** Its value is a stable view across many PRs, which code
work would erode.

**Worker.** One session, one brief, one PR. Boots from the read order above,
does the work, leaves `docs/state.md` updated and the brief's "Outcome"
section filled in. A worker that finishes early stops; it does not pick up
the next roadmap item on its own.

**Reviewer (optional, for risky PRs).** A fresh session or subagent that has
not seen the work being produced. Gets the brief and the diff, nothing else,
and answers: does the diff do what the brief says, does it break an
invariant, is anything in the brief's "out of scope" list touched?

## Coherence budget — when to stop and hand off

A session hands off instead of pushing on when **any** of these is true:

- it has merged or opened one PR (workers) or ~5 PRs / one phase (orchestrators);
- it notices it is re-reading files it already read, or re-deciding something
  it already decided;
- its context is past roughly half full;
- the work has turned out to be a different shape than the brief described.

The last one is the most important. A brief that was wrong is not a failure;
silently doing different work than the brief describes is. Stop, write what
you found into the brief's Outcome, and hand back.

**Handoff = three writes, then stop:**

1. `docs/state.md` — overwrite the snapshot (branch, PR, CI, next step).
2. The brief's `## Outcome` — what landed, what didn't, what you learned that
   the next session would otherwise rediscover the hard way.
3. Push. A handoff that isn't pushed didn't happen.

Orchestrators rotate too: at each phase gate, the orchestrator writes the
next phase's briefs, updates state, and ends. The next phase starts with a
fresh orchestrator booted only from the files. This is deliberate — if a
fresh session can't pick up from the files, the files are the bug.

## Subagents

Use sparingly. A subagent starts cold and re-derives context, so it only pays
when the question is broad and the answer is narrow.

- Give it a **bounded question with a bounded answer** ("which systems read
  `GridSettings.spacing`, with file:line"), not a task ("fix snapping").
- Keep its input under ~40% of its context: name the files or crates to
  look in, tell it to read excerpts not whole files, and tell it the answer
  format and length.
- Never have a subagent write code that a worker then has to trust blind.
  Code is written in the session that will own the PR.
- Good uses: fan-out searches across crates, a cold-eyes review of a diff
  against its brief, checking a decision record for a contradiction.

## Briefs

One file per unit of work in `docs/briefs/`, named `<phase>-<n>-<slug>.md`.
Template:

```markdown
# <Title>
Phase · roadmap item (quote it) · status: ready | in progress | done | returned

## Goal          — one paragraph, the observable outcome.
## Read first    — 3–6 files/sections, with why. Nothing else is required.
## Facts         — what the orchestrator already found, with file:line, so
                   the worker doesn't re-derive it. Mark anything unverified.
## Approach      — the intended shape, and the invariants it touches.
## Out of scope  — explicit list. Touching these = stop and hand back.
## Done when     — tests that must exist, gates, doc updates.
## Outcome       — filled by the worker at handoff.
```

A good brief is sized so a worker can finish it with half its context to
spare. If a brief needs more than ~6 "read first" entries, split it.

## Gates and CI

Workers run, every change: `cargo fmt --all -- --check`, clippy, doc, and
the per-package tests for crates they touched. The full
`cargo test --workspace` is run **once**, at the end, as the gate. See the
"Agent container" section of `CLAUDE.md` for disk recovery.

CI is the authority, and it uses `@stable`, which is usually newer than the
container's toolchain. Agent sessions **cannot download Actions logs**
(403), so CI re-emits clippy diagnostics as annotations. To read a failure:

```sh
gh api repos/benbrandt14/gradiance/commits/<branch>/check-runs \
  --jq '.check_runs[] | "\(.id) \(.name) \(.conclusion)"'
gh api repos/benbrandt14/gradiance/check-runs/<id>/annotations \
  --jq '.[] | .message'
```

GitHub GraphQL is unavailable from sessions; `gh pr list` fails. Use REST
(`gh api repos/{o}/{r}/pulls`). PR CI runs only on pull requests, so open a
draft PR early to get a CI read.

## The plan: resolving the existing roadmap

Phases are ordered by dependency and by the future intent below. Each phase
ends at a **gate**: main green, `state.md` current, the orchestrator rotates.
Item numbers in parentheses are the roadmap's feedback references.

**Phase 0 — Reset.** Green main on current stable; this document;
`state.md`; CI annotations; the `frames-decision.md` stub. *Gate:* main CI
green.

**Phase 1 — M18 grids & snapping.** Small, well-specified, test-friendly —
the shakedown of the orchestration loop itself.
1. Snap to the adaptive grid step (the known defect).
2. Major/minor lines (shares step 1's function).
3. Axis-lock basis follows the active grid system.
4. Snap glyph stability, tangent glyph, snap-off-when-hidden.
5. Light collinear/centreline snapping.
6. Alternate constructors (3-point box, tangent circle).
Curvilinear grid coordinates (polar edges curve) is **not** in this phase —
it is a design question about what a tool's coordinate space is, and it
touches the same seam as simulation planes. Write it up as a decision note
during Phase 5 instead.

**Phase 2 — Constraints & substrate completion.** Finishes the sim substrate
the scripting phases bind to.
- M16 residual: joint config in the context menu; required components on the
  authored `Body`/`Joint` archetype.
- M20 remainder: breaking limits first (they produce a queryable quantity),
  then backlash, cams/planar contact as separate briefs.
- Joint sensor sources (length/angle as `SignalSource`s) so joints plot.
- Decide PR #194 (rigid rod + elastica flexure spike): salvage the `Capsule`
  leaf and the Spring/Strut split into briefs, or close it with a note.

**Phase 3 — Scripting & the op surface.** Highest leverage for the future
intent, because an external UI and MCP tooling both bind to the op registry.
- Fuel/step budget for runaway scripts (P0/P1 remainder) — first, since
  everything after it runs user code.
- `group`/`ungroup` via list argument; `register-action` receives click
  point; fold hard-coded context-menu buttons into the action table.
- `reflect_functions` + an app-level `bevy_remote` (BRP) seam exposing reads
  and `gradiance.op.*` (adoption audit rows 7/17). This is the MCP door —
  design it as the *external* control surface, not a debug toy.
- User tools from `.scm`.

**Phase 4 — Geometry & render.** M21 CSG booleans and fillet; M19 rendering
items that are render work (emissive, AO/contact shadows, body borders).
**M19's chrome items** (palette picker, camera settings section, sim-settings
widgets) are built *functional, not polished* — see below.

**Phase 5 — Units & frames, the 3D on-ramp.** Units P2·types → P3 → P4 → P6.
Then write `docs/frames-decision.md` properly, and the curvilinear-tools
decision note from Phase 1. These are the seams simulation planes consume.

**Phase 6 — Testing & diagnostics.** `insta` golden + intent-replay
snapshots, polygonize proptests, the diagnostics stack (audit section B).
Some of this may be pulled earlier if a phase needs it; that's fine.

**Deferred to the next planning stage** (not part of "roadmap complete"):
`bevy_picking` interaction rework (audit D); dock and node-canvas layout
persistence; `bevy-persistent`; `egui_code_editor`; the UI framework
question in `ui-shell-decision.md`; PR #208.

## Future intent — do not act on, do not foreclose

Two directions follow the roadmap. Neither is in scope now. Every phase
above should leave them cheaper, never more expensive.

**1. The UI will be bootstrapped from an external project** that already
has UI test and MCP infrastructure; editor and UI code will be copied in.
So, until then:

- **Invest in the op surface, not the chrome.** A feature is done when it is
  reachable as an intent and a registered op with a test; its egui panel
  should be the thinnest thing that exercises it. Don't polish egui widgets,
  layouts, or theming beyond usable.
- **Keep `gradiance-ui` a consumer.** No logic in UI that a script or
  external client would need. If a panel computes something, that
  computation belongs in a lower crate behind a read.
- **Keep the UI replaceable at the crate boundary.** The rule that `egui`
  lives only in `gradiance-ui` is what makes a swap possible; protect it.
  Menu/View tables (`Panels::named`, the action table) are data a new
  shell can read — prefer extending those over hand-wiring.
- **BRP/MCP readiness** (Phase 3) is the bridge an external project's
  tooling will plug into.

**2. 3D via "simulation planes."** The 2.5D model generalizes to multiple
simulation planes, then real 3D. PR #208 (draft, July: avian2d →
bevy_rapier3d, groundwork for planes) is parked input to that planning stage
— don't merge, rebase, or close it. Meanwhile:

- **Don't add new single-plane assumptions.** Code that needs "the plane"
  should get it from one place (`INTERACTION_PLANE_Z`, `DepthBand`, the
  grid's frame) rather than hardcoding z or XY. If you add a new one, name it.
- **Typed units and frames are 3D enablers** — Phase 5 is on-ramp, not polish.
- **Keep physics reads behind `physics::queries`** so swapping the engine
  later is a facade change, not a crate-wide edit.

When a brief would make either direction harder, the brief says so and the
orchestrator decides. Workers don't make that trade silently.

## When the roadmap is done

The final orchestrator writes a short retrospective in `docs/state.md`
(what's left in "deferred", what the op surface looks like, which seams are
plane-agnostic) and stops. The next planning stage — UI bootstrap and
simulation planes — starts from that.
