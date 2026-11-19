# Frames and tool coordinates: decision proposals
Phase 5 · "Then write `docs/frames-decision.md` properly, and the
curvilinear-tools decision note from Phase 1. These are the seams
simulation planes consume." (`docs/orchestration.md` § Phase 5) ·
status: in review

## Goal

Two decision documents, written as **proposals for the owner to ratify**.
This is not code.

1. `docs/frames-decision.md` becomes a real decision record:
   - the question (already stated there);
   - the options;
   - a recommendation with its trade-offs;
   - which seams change, which stay;
   - a phased plan in the style of `docs/units-decision.md`.
2. A curvilinear-tools note, its own file or a section of (1), answering:
   do tools operate in grid coordinates? For example, does a polygon edge
   drawn in a polar grid curve?

Both are written with the consumer in mind: **multiple simulation planes,
then 3D** (`docs/orchestration.md` § Future intent 2). The recommendation
must make that direction cheaper, not merely support CAD datums.

## Read first

- `docs/frames-decision.md` (the question and the seams held open).
- `docs/units-decision.md` § Coordinate frames, § Forward compatibility.
- `docs/roadmap.md` § Horizon (CAD kernel front-end, Full 3D) and M18's
  curvilinear line (≈L284).
- `docs/orchestration.md` § Future intent 2.
- PR #208 (draft, parked: **read only**, never rebase/merge/close): its
  frame / simulation-plane model. `gh api
  repos/benbrandt14/gradiance/pulls/208/files`.
- Code that is a frame in all but name:
  - `GridSettings` origin/rotation ("Align grid to body");
  - `DepthBand`;
  - `INTERACTION_PLANE_Z`;
  - `units::world` / `WorldScale`;
  - the snap/axis-lock basis (brief 1-3).

## Facts (orchestrator, 2026-11-16)

- A decision record here is ratified by the owner, not by a worker or the
  orchestrator. Mark both documents **"Status: proposed"**. The orchestrator
  presents the recommendation to the owner; nothing is built on it until
  then.
- *Unverified:* how many places hardcode the XY plane or z (the "single-plane
  assumptions" Future intent 2 says not to add). An inventory with file:line
  is part of the deliverable, because it sizes every option.

## Approach

1. Inventory the single-plane assumptions and the existing frame-like
   seams (a table).
2. Options: at least (a) frames as a `units`-level transform type composed
   at `WorldScale`, (b) frames as authored entities (datums) bodies
   reference by `StableId`, (c) defer until the 3D engine decision. For
   each, give its cost, what it unlocks for simulation planes, and its
   effect on the save format.
3. The curvilinear question as the interaction-layer face of the same
   choice: what a tool's coordinate space is under each option.
4. A recommendation and a phased plan with no dates.

## Out of scope

Any code change, choosing the 3D engine (PR #208's question), new briefs,
`docs/state.md`.

## Done when

- `docs/frames-decision.md` rewritten as a proposal; the curvilinear note
  written; both linked from `units-decision.md` and the roadmap lines that
  reference them. The inventory table is included.
- The PR description lists the open questions the owner must answer.
  Outcome filled.

## Outcome

**Landed (docs only):** `docs/frames-decision.md` rewritten as a decision
record (question, terms, inventory, options (a)/(b)/(c), recommendation,
seams that change and stay, phases F0–F5) and a new
`docs/curvilinear-tools-decision.md`. Both are linked from
`docs/units-decision.md` (§ 4, § Extensibility contract, § Forward
compatibility, § Phasing) and from `docs/roadmap.md` (M18 curvilinear line,
the units checklist, § Horizon CAD and Full 3D).

**Status override.** The Facts said "Status: proposed", ratified by the
owner. The owner ruling of 2026-11-17 (`docs/orchestration.md` § Review
mode) makes architecture options a session decision on best estimate. Both
documents say "Status: decided (session best estimate, 2026-11-19); the
owner may reverse it", and the choices are `decision: true` entries in
`docs/review/archive.ron`: `frames-option` (picked (a)) and
`curvilinear-edges` (picked straight edges). No review question was added:
no choice here can be judged in a live scene yet.

**Decision.** (a) now: a `PlaneFrame` value type in `gradiance-core` at
the 2D↔3D lift, ported from #208, with `PosRot` as the in-plane frame and a
CI ratchet on hand-built z/XY sites; all authored, saved, queried and
scripted state stays plane-local 2D. (b) (planes and datums as authored
entities, referenced by `StableId`, not by #208's `u8` index) waits for its
first consumer; (c) is rejected because the engine decision needs the seam
first. Tools compute in plane-local 2D, points snap through the grid's
chart, and edges stay straight: a polygon edge in a polar grid does not
curve. An arc-on-a-ring constructor and a radial/ring lock are a later
option.

**Inventory.** 60 sites in 23 Rust files build or drop the third axis by
hand (51 unrouted; 6 go through `INTERACTION_PLANE_Z`, 3 are `PosRot`'s
seam), plus 1 shader line (`plane.wesl:123`), 78 `*_2d` gizmo calls in 7
files, and the `avian2d` engine (8 manifests, 27 source files). The table
has 24 rows: 16 single-plane assumptions and 8 frame-like seams.

**Learned (would be rediscovered otherwise):**

- There is no `WorldScale` type. Since the P2 flip `units::world` is only
  the camera's orthographic scale (`render/lib.rs:117`,
  `src/review.rs:471`). Docs that said "frames compose at `WorldScale`"
  now point at the lift.
- `camera::plane_point` intersects a literal `z = 0`, not
  `INTERACTION_PLANE_Z`: the one picking seam bypasses the named constant.
- 8 sites convert body-local ↔ world by hand with
  `affine.transform_point3(v.extend(0.0)).truncate()`; a `PosRot`
  helper pair would remove them (phase F2).

**Not done (out of scope):** no code; no new briefs (F1–F5 become briefs
at the next planning stage); the engine choice (#208) untouched.
