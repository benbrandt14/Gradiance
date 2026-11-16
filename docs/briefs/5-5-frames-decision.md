# Frames and tool coordinates: decision proposals
Phase 5 · "Then write `docs/frames-decision.md` properly, and the
curvilinear-tools decision note from Phase 1. These are the seams
simulation planes consume." (`docs/orchestration.md` § Phase 5) ·
status: ready (after 5-1–5-4)

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

*(worker fills in)*
