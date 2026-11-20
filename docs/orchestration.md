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

CI is the authority. Its toolchain is pinned (`RUST_PIN` in `ci.yml`) to
the version agent containers have, so local gates should match it; bumping
the pin is an orchestrator-scheduled brief, never a side effect. Actions
logs used to 403 for agent sessions (`gh run view <id> --log-failed` works
as of 2026-11-12), so CI also re-emits clippy
diagnostics and **test failures** (failed test names, panic messages, and
the output tail) as annotations. To read a failure:

```sh
gh api repos/benbrandt14/gradiance/commits/<branch>/check-runs \
  --jq '.check_runs[] | "\(.id) \(.name) \(.conclusion)"'
gh api repos/benbrandt14/gradiance/check-runs/<id>/annotations \
  --jq '.[] | .message'
```

GitHub GraphQL is unavailable from sessions; `gh pr list` fails. Use REST
(`gh api repos/{o}/{r}/pulls`). PR CI runs only on pull requests, so open a
draft PR early to get a CI read.

## Review mode — questions for the owner

Product decisions reach the owner **in the running app, not in chat**
(owner direction 2026-11-16, brief 4b-0). The owner runs

```sh
cargo run -- --review            # docs/review/questions.ron
```

flips between each question's options (or views them side by side), plays
with the live scenes, and saves a choice plus a note. The app writes
`docs/review/answers.ron`; the owner commits it. Schema:
`gradiance_scene::review`.

**Asking (orchestrators and workers).** Never ask a product question in
chat or leave it in an Outcome as "owner to decide". Add it to
`docs/review/questions.ron` instead, in the same PR:

- a stable `id` (lower-case, `-`), a title, a description that says why it
  matters and what each choice implies, and `source` (brief / `state.md`
  item);
- 2–4 options, each with a label, a caption saying what to try, and a real
  scene under `docs/review/scenes/<id>/`. Prefer `.scm` scripts (they
  survive `FORMAT_VERSION` bumps; a `.ron` scene does not). An option's
  scripts run one frame apart, so a second script can `body-index-at`
  what the first spawned;
- settings differences go in the option's `settings` override (A/B only);
  set `side_by_side: true` only when options differ in bodies, not settings;
- add the question's expected body counts to `tests/it/review.rs`
  (`expected_bodies`) — the guard loads every option and fails on a script
  error or a wrong count.

A **product** question whose alternative cannot be shown yet (it needs code
that does not exist) is not a review question: record it as a follow-up
until a scene can show both sides.

**Plan decisions are not asked** (owner, 2026-11-17: "Leave non
interactive changes to the thread here. Go with your best estimates").
Review mode is for **interactive** questions: a choice the owner judges in a
live scene. A choice that no scene can show (an order of work, an
architecture option, a process rule) is made by the session on its best
estimate. Record it so that the owner can find and reverse it:

- append an entry to `docs/review/archive.ron` with a `decision: true`
  question (text-only options: a label and a caption that says what the
  option does and what it costs), the picked option, a note that the
  session decided, and `ingested_as` = the brief or doc that holds the
  result;
- small choices inside a brief's own scope go in its Outcome, not here.

Ask the owner in chat, and wait, only where a step cannot be undone.

**Ingesting (orchestrator, at each phase start).**

1. Read `docs/review/answers.ron`. For each answer:
   - turn it into the roadmap/brief change it implies (a new brief, a scope
     change in a planned one, or a `state.md` ruling);
   - move the question and its answer to `docs/review/archive.ron`
     (`Archive` schema: `question`, `answer`, `ingested_as` = what it
     became), and delete its entry from `questions.ron` and `answers.ron`.
     Keep its `scenes/<id>/` files; the archive entry still points at them,
     and drop its row from `expected_bodies` (the three seed rows stay: the
     mechanism tests run over `tests/fixtures/review/questions.ron`, which
     uses those scenes).
2. An answer of "none of these" is still an answer: its free text and note
   are the owner's direction; if it opens a new question, ask it as a new
   question with scenes.
3. Commit the ingestion as its own change ("review: ingest <ids>").

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

**Phase 4b — Owner direction (2026-11-16).** Groups as editing layers
(4b-0 review mode — open questions as live scenes the owner answers in the
app, answers committed and ingested at each phase start; 4b-1 op surface: names, group-scoped edits, distribute within bounds,
filters, decompose; 4b-2 the panel), one art direction with selectable
presets and SSAO options (4b-3), and parameters scaled to the body so
defaults behave and only slider extremes misbehave (4b-4), each locked in
with behaviour tests. The owner also ruled that the **save format and UI may
change freely** while the project is experimental: bump `FORMAT_VERSION`,
reject older files, regenerate goldens; no migrations.

**Phase 5 — Units & frames, the 3D on-ramp.** Units P2·types → P3 → P4 → P6.
Then write `docs/frames-decision.md` properly, and the curvilinear-tools
decision note from Phase 1. These are the seams simulation planes consume.

**Phase 6 — Testing & diagnostics.** `insta` golden + intent-replay
snapshots, polygonize proptests, the diagnostics stack (audit section B).
Some of this may be pulled earlier if a phase needs it; that's fine.

**Recurring — toolchain bump.** Not a phase; a brief the orchestrator
schedules at a phase gate when a newer stable is wanted (containers install
any version through `rust-toolchain.toml`). Bump `rust-toolchain.toml` and
`RUST_PIN` together, fix the new lints in the same PR. Last bump: D-0
(1.97.0 → 1.100.0, 2026-11-17); it cleared the `assert!(x.is_empty())` debt.
No known debt for the next bump.

**Phase U — one operation surface (owner direction 2026-11-17).** Plan:
`docs/unified-surface-plan.md`. Toolchain and Bevy 0.20 upgrades (D-0,
D-1), one declaration per operation projected to Lisp, BRP/MCP, CLI, menus,
keys and docs with conformance tests (U-1, U-2), in-app test and control
(U-3..U-6), then Blender-leaning polish (U-7..U-11). Its briefs interleave
with 4b and 5; the order is in `docs/state.md`.

**Phase S — sandbox parity (audit 2026-11-20).** Gaps from
`docs/parity-audit.md`, briefs S-1..S-9 in number order (roadmap §
"Sandbox parity"). Runs after Phase Q; independent of G-0 and 3D-0. S-8
starts with an API check and may return.

**Deferred to the next planning stage** (not part of "roadmap complete"):
`bevy_picking` interaction rework (audit D); node-canvas layout persistence
(becomes document content if the graph becomes the document: G-0);
`bevy-persistent`; `egui_code_editor`; PR #208. *Revised 2026-11-17:* dock
layout persistence moved into U-10 (workstation config); the UI framework
question is settled (egui, polished).

## Future intent — do not act on, do not foreclose

Two directions follow the roadmap. Neither is in scope now. Every phase
above should leave them cheaper, never more expensive.

**1. The UI will be bootstrapped from an external project** that already
has UI test and MCP infrastructure; editor and UI code will be copied in.
So, until then:

- **Op surface first, then excellent chrome.** A feature is done when it is
  reachable as an intent and a registered op with a test. *Revised
  2026-11-17:* the owner now wants excellent egui polish, leaning to
  Blender's theme (`docs/unified-surface-plan.md` § 0, § 7). Polish lands
  through the U-briefs, after the op registry (U-1, U-2), so every panel is a
  projection of registered operations.
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
