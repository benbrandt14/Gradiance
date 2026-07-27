# Plan: fold sketch mode into the paused editor

**Status:** proposed, not started. Written after the sketch-mode spike landed
(`docs/solvespace-sourcing-decision.md`, `crates/gradiance-sketch`,
`interaction::tools::sketch_session`, `ui::sketch_panel`).

## The target

There is no separate sketch mode. **Paused *is* the CAD surface**; playing runs
the simulation. Box and circle are sketch tools that happen to be fast paths,
polygon is a closed loop of lines, and every one of them draws into the same
constrained document with the same snapping. `EditorMode` goes away;
`GameState::{Playing, Paused}` is the only modality left.

A separate pass — explicitly out of scope here — then defines joint, motor and
spring behaviour in that CAD context. This plan's job is to make the sketch
substrate good enough that that pass is worth doing.

---

## What is already true

Worth stating, because it decides how much of this is new work:

- One session, one document, one selection (`sketch_session`), with Select /
  Line / Arc / Circle / Trim, live constrained dragging, and a constraint panel
  driven by `edit::applicable`.
- Committed bodies carry their `SketchDoc`, can be re-opened by clicking, and
  reshape in place through `ReshapeBodyIntent` keeping their `StableId`.
- Sketch-internal snapping (`pick.rs`) already resolves point / midpoint /
  centre / on-entity, and snapping already creates real *constraints* rather
  than just coordinates.
- The solver is a pristine vendored SolveSpace behind our own bindings, with the
  full 38-constraint vocabulary exposed.

So the unification is mostly about **removing a boundary**, not building a
second system. The genuinely new work is in stages 0 and 3.

---

## Stage 0 — three prerequisites

These are independent, each useful on its own, and each blocks something later.
None of them require the mode collapse to have happened.

### 0a. Lowering must preserve analytic primitives

**This is the crux of the whole plan, and the one place it can quietly go
wrong.**

`lower.rs` currently produces `ShapeDef::Polygon` for everything. That was the
right call for a spike — it meant colliders, meshes, snapping and rendering
needed no changes at all. But if box and circle become sketch tools while
lowering stays polygon-only, then **every circle in Gradiance silently becomes a
48-gon**: no longer an exact SDF, more expensive to evaluate, visibly faceted
under zoom, and different under CSG. That is a real regression dressed up as a
refactor, and it would land on existing content the moment the direct tools
retire.

Fix lowering to recognise what it is looking at:

| Sketch content | Lowers to |
|---|---|
| exactly one `Circle` entity | `ShapeDef::Circle` |
| a closed 4-line loop, axis-aligned, opposite sides equal | `ShapeDef::Box` |
| anything else closed | `ShapeDef::Polygon` (as today) |

Recognition keys off the *solved geometry*, not off which tool drew it, so a
polygon dragged into a rectangle becomes a `Box` and a `Box` stretched off-axis
degrades to `Polygon` on its own. That is the parametric behaviour, and it is
also what keeps the fast paths honest: "circle tool" stops being a different
kind of object and becomes a shortcut for a sketch that happens to be one
circle.

Tests: each recognition rule, plus the degradation direction (off-axis box →
polygon), plus a round-trip that a `Circle`-lowered body re-opens to a sketch
containing one circle.

**If this stage is skipped or deferred, stage 2 must not ship.**

### 0b. One snap resolver over both worlds

There are two snapping systems today and they do not know about each other:

- `interaction::snap::SnapKind` — Grid, Vertex, Midpoint, Center, Edge, over
  **world bodies**, producing a snapped *cursor position*.
- `sketch::pick::SnapKind` — Point, Midpoint, Center, OnEntity, over the
  **sketch document**, producing a snap that becomes a real *constraint*.

Unified, drawing a line has to be able to snap to a neighbouring body's corner
and to the sketch's own geometry in the same gesture, with one priority order.
Today it can only see the document.

The shape of the fix: keep the two candidate producers (they read different
data), merge their results through one comparison, and keep the distinction that
matters — a hit on sketch geometry can carry a constraint, a hit on a foreign
body carries only a position, *until* stage 3 gives foreign geometry a
constraint story. Do not merge the two `SnapKind` enums into one; they mean
different things and collapsing them would lose the "can this become a
constraint" bit.

### 0c. A real fixed joint

The user-facing ask is "a single line acts like a non-colliding weld joint, to
preserve it during play". Gradiance has no such thing today:

- `JointKind` is `Hinge | Slider | Spring`. There is no fixed/weld variant.
- `ToolState::Weld` is **not a joint** — it merges two bodies into one SDF
  union, or pins a single body static (`connector_tool.rs:8`). Merging fuses
  geometry, which is the opposite of "preserve the line as a link".

So: add `JointKind::Weld` mapping to avian's `FixedJoint`, defaulting to
`collide_connected: false`. This is a small, self-contained addition to
`domain::joint` + `physics::joint_sync` + the joint inspector, and it is a
capability worth having regardless of the sketch work.

Naming hazard: `ToolState::Weld` already means "merge". Either rename the
existing tool to Merge (it is what it does, and the docs already say so) or name
the new joint `Fixed`. Recommend renaming the tool to **Merge** and calling the
joint **Weld**, because "weld" reads as a link to everyone who has used CAD, and
the merge tool's own doc comment already apologises for the name.

---

## Stage 1 — collapse the mode

Mechanical once stage 0 is in, and independently shippable.

- Delete `EditorMode`. `GameState::Paused` becomes the sketching surface.
- Merge `SketchTool` into `ToolState`. The `.run_if(in_state(EditorMode::Direct))`
  gate on the direct-tool tuple becomes `in_state(GameState::Paused)`.
- `Drag` (fling a body physically) is the one tool that only means anything
  while *playing*; it gates the other way.
- `pause_for_sketch` / `resume_after_sketch` / `ResumeAfterSketch` all delete —
  there is no mode to enter, so there is nothing to remember and restore.
- The sketch session stops being modal: it holds whatever document the author is
  currently editing, and is empty the rest of the time. `sketch_panel` shows
  when the session is non-empty rather than when a mode is active.
- Hotkeys stop being mode-dependent, which removes `sketch_shortcuts` and the
  `EditorMode` gate added to `apply_shortcuts`.

Migration: none needed. `EditorMode` was never persisted — it is editor state,
not scene content.

---

## Stage 2 — retire the direct draw tools

With 0a done, `box_tool` / `circle_tool` / `polygon_tool` are strictly worse
versions of the sketch tools: same output shape, but no constraints, no
re-opening, no dimensions.

- Box becomes a sketch tool: drag a rectangle, emit four lines with
  Horizontal/Vertical/EqualLength constraints already attached, and let 0a
  lower it back to `ShapeDef::Box`. Dragging a corner afterwards keeps it
  rectangular *because of the constraints*, which is the whole argument.
- Circle becomes today's sketch circle.
- Polygon becomes the line tool with loop closing — it already is.
- `ground_tool` stays as-is. `HalfPlane` is unbounded and has no closed profile,
  so it is not expressible as a sketch and should not be forced into one.
- `cut_tool`, `drag_tool`, `select`, the connector tools and `node_tools` are
  untouched — none of them author profile geometry.

Existing scenes keep working: a body with no `SketchDoc` renders, simulates and
edits exactly as now. It simply cannot be re-opened for constraint editing,
which is already true today. **Do not** attempt to back-fill sketches onto
existing bodies — a reconstructed sketch would invent constraints the author
never asked for, and would be wrong the first time they dragged it.

---

## Stage 3 — a lone line as a link

The deepest stage, and the one with a real architectural question in it.

Today a line that is not part of a closed profile is dropped at commit — it has
no area, so it cannot be a body. The target is that such a line survives into
play as a rigid, non-colliding connection between the two things its endpoints
land on.

For that, a sketch endpoint has to be able to *name a body*. That is an
**assembly constraint**, and the sketch crate currently has no notion of a body
at all — deliberately: `sketch → core, geometry, slvs-sys`, with **no physics
edge**, and `tests/boundaries.rs` enforces it.

The resolution that keeps the layer rule: the sketch document gains an opaque
anchor — a point may carry an `Option<StableId>` it does not interpret. To the
sketch crate it is a foreign key it never dereferences; the solver still sees a
plain fixed point. Resolving that id to an entity, reading its transform, and
building the joint all happen in `interaction`/`command`, which already depend
on both sides. `StableId` lives in `core`, which `sketch` already depends on, so
this needs no new DAG edge and no physics edge — the invariant survives intact.

Commit then splits by what it finds:

| Sketch content | Commits as |
|---|---|
| closed profile(s) | body, as today |
| a line anchored to two bodies | `JointKind::Weld`, `collide_connected: false` |
| a line anchored to one body | that body pinned static, or a weld to the world |
| an unanchored lone line | nothing, with a status line saying why |

Open questions to settle before writing this, not now:

- What happens to a sketch containing *both* a closed profile and anchored
  lines? Probably one body plus its links, committed together — but that is more
  than one command, and the one-gesture-one-command invariant needs an explicit
  answer (most likely a single composite command, the way `ReshapeBodyCommand`
  bundles shape + sketch + transform).
- Does the anchored line stay live after commit — i.e. does dragging the body
  in play re-solve the sketch? Almost certainly not: the solver must never run
  in the per-frame loop (the Tier-A/Tier-B rule in CLAUDE.md). The sketch is
  authoring-time; the joint is the runtime artefact.
- Do anchors survive the body being deleted? The joint machinery already has an
  answer for dangling references; the sketch document needs the same one.

---

## Sequencing and risk

```
0a lowering ──┐
0b snapping ──┼── 1 collapse mode ── 2 retire draw tools
0c weld joint ─────────────────────────────────── 3 lone lines ── (separate pass: joints/motors/springs)
```

- **0a is the only stage that can silently damage existing content.** Everything
  else either fails loudly or is additive.
- Stages 0 and 1 are safe to ship incrementally behind no flag; each leaves the
  editor working.
- Stage 2 is the one users will feel. Worth landing on its own so the diff is
  reviewable as a behaviour change rather than buried in a refactor.
- Stage 3 should not start until 0c exists and stage 2 has been used in anger —
  its open questions are much easier to answer with a unified editor to try
  them in.

## What this plan deliberately does not do

- Joint/motor/spring authoring in the CAD context. Explicitly a separate pass,
  and it wants stages 0–3 finished first.
- Back-filling sketches onto existing bodies.
- 3D, or moving the sketch off its single workplane. `doc`/`solve` stay
  dimension-agnostic and `lower` stays the only 2D-specific module, so that
  remains a sibling module rather than a rewrite.
