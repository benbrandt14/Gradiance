# Boolean operations between bodies
Phase 4 · M21 · "Boolean operations between bodies via context menu (join /
subtract / intersect / xor) producing analytic trees (7.3)" · status: ready

## Goal

With two or more bodies selected, the user can **subtract**, **intersect** or
**xor** them, as well as join them (join already exists as Merge). The result
is an analytic SDF tree on the host body, exactly as Merge builds a `Union`
today. Each operation is:
- one intent;
- one command;
- one undo step;
- a registered verb;
- a context-menu entry.

## Read first

- `crates/gradiance-command/src/merge_cmd.rs`. The model to generalise:
  - the host is the first target;
  - the others are placed in the host frame;
  - joints are rewired;
  - deep trees bake to a polygon at `MAX_CSG_DEPTH`;
  - a disconnected result is refused.
- `crates/gradiance-command/src/cut_cmd.rs`: how a body that splits into
  pieces is replaced by polygon-leaf pieces and joints re-anchored
  (`rewire_joints`).
- `crates/gradiance-geometry/src/shape.rs` (`CsgOp`, `MAX_CSG_DEPTH`) and
  `sdf.rs` (field eval and bounds per op).
- `crates/gradiance-ui/src/context_menu.rs` ≈L515 ("Merge into one body")
  and `crates/gradiance-script/src/registry.rs` ≈L83 (`merge`).
- `docs/sdf-geometry-decision.md`, `docs/scripting.md` § adding a verb.

## Facts (orchestrator, 2026-11-14, main `294a47d`)

- `CsgOp` has `Union`, `Subtract`, `Intersect` and `SmoothUnion { radius }`.
  There is no `Xor`. Xor can be written with the existing ops as
  `Subtract(Union(a,b), Intersect(a,b))`, at the cost of tree depth, or added
  as a variant, which changes the save content. Pick one and say why in the
  Outcome. A new variant needs the scene format reviewed (`gradiance-scene`
  version and migrations).
- Merge refuses a disconnected result because grouping is the tool for
  rigid links. Subtract and xor can legitimately split a body. *Unverified:*
  whether reusing cut's split path (pieces as new bodies, inheriting
  props/appearance/field source) is clean, or whether refusing
  disconnected results is the better first cut. Decide, test it, and record
  it.
- What happens to the non-host operands is a design choice. Merge despawns
  them. For subtract, the CAD convention is that the tool body is consumed.
  Document the rule you pick, and keep it identical across menu, verb and
  undo.

## Approach

1. Generalise the merge intent/command, either with an op field on one
   boolean intent (Merge = Union) or with sibling intents. Prefer one row in
   `command_intents!` if it stays readable. Keep `MergeIntent`'s existing
   behaviour and tests intact.
2. Add verbs (e.g. `(subtract a b)`, `(intersect a b)`, `(xor a b)`, indexed
   like `merge`), each with an `OpSpec` row.
3. Add context-menu entries next to "Merge into one body" for a
   multi-selection, built-in and intent-emitting. If the 3-3 action table
   fits, render them from it; otherwise wire them as Merge is, and say why.
4. Add `gradiance-geometry` unit tests for field and bounds of any new op.

## Out of scope

Smooth/filleted operations (brief 4-2), mesh booleans, ops on the ground
half-plane (refuse, as Merge does), selection-scoped verbs, `docs/state.md`.

## Done when

- Integration tests for each op:
  - the host's shape is the expected tree, checked by containment at
    sample points;
  - operands are handled per the documented rule;
  - joints are rewired;
  - one undo restores everything, records equal.
- A test for the disconnected-result decision.
- Screenshot via `tools/screenshot/`: a box minus a circle, and an
  intersect. Set them up with startup scripts, which can call the new verbs.
  Describe them in the PR, keep them out of git, and report the paths.
- `docs/scripting.md` verbs, and the roadmap M21 line marked landed. Gates
  green. Outcome filled.

## Outcome

*(worker fills in)*
