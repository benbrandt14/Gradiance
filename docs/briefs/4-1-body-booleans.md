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

**Status: done.** Subtract, intersect and xor between bodies are one
intent, one command, one undo step, three verbs and three context-menu
entries; merge is unchanged.

**Intent shape.** One new row in `command_intents!`:
`BooleanIntent { op: CsgOp, targets }` → `MergeCommand::with_op`.
`MergeIntent` keeps its row and its behaviour (`MergeCommand::new` = union),
because the weld tool, the `merge` verb and existing tests emit it. The op
is the geometry's own `CsgOp` (it now derives `Reflect`), so brief 4-2 can
emit `CsgOp::SmoothUnion { radius }` through the same intent without a new
row. Undo labels follow the op: `merge` / `subtract` / `intersect` / `xor`.

**Xor representation: a new `CsgOp::Xor` variant**, field
`max(min(a, b), -max(a, b))`, bounds = the union's box. The composition
`Subtract(Union(a,b), Intersect(a,b))` copies both operands, so a chain of
k xors would grow the tree as 2^k and hit `MAX_CSG_DEPTH` after a few.
The variant gives the same field (unit test checks it point-by-point against
the composition) at one node. **Scene format:** additive within v6 (no
version bump). Every v6 file still loads; a file that uses xor fails to parse
("unknown variant") in an older build. Recorded on `FORMAT_VERSION`. The
persistence round-trip proptest now generates `Xor`.

**Operand rule (same on menu, verb, weld tool, undo).** The first target
(first-selected body; `a` in `(op a b)`) hosts the result and keeps its id,
pose, props, and groups. **Every other operand is consumed** (despawned), as
merge already did and as CAD does with a tool body. Operands fold left to
right: `((host op b) op c) …`.

**Disconnected results.** Union (and smooth union) still refuse anything but
one connected body. Grouping is the tool for a rigid link. Subtract, intersect
and xor **split**: the host is replaced by polygon-leaf pieces through cut's
path. `cut_cmd::piece_records` and `replace_with_pieces` were factored out of
`CutCommand` so both share them. Pieces get fresh ids and inherit props,
appearance, depth, groups, field, tracer and the velocity `v + ω × r`.
Refusing was not workable for xor. Two partially overlapping bodies always
xor into separate pieces, because A∖B and B∖A meet only at boundary
crossings, so a refuse-disconnected xor would only ever work for one body
containing the other, where it equals subtract.
**Refused** (`NoEffect`, no history):
- an empty result (an intersect of disjoint bodies, or a subtract that
  swallows the host);
- a non-union operand whose bounds miss the host (it would only delete the
  operand);
- duplicate targets, fewer than two targets, or a ground half-plane.

**Joints.** Joints between two targets become internal and are deleted. An
end on a consumed body re-anchors onto the host at the same world point,
taking merge's `rest_rot` convention. An end whose anchor was on its body's
material and is outside the result is **deleted**, as a cut deletes joints
whose material is destroyed. A union never removes material, so merge
behaves exactly as before. When the host splits, each end moves to the piece
that contains its anchor, or the joint is deleted, as in cut's `rewire_joints`.

**Menu wiring.** The entries sit directly after "Merge into one body":
"Subtract from first selected", "Intersect" and "Xor". Each has a hover
hint and is enabled for ≥ 2 selected. They are wired as Merge is, not from
the 3-3 action table. The table holds script text with a click point, and
these actions need the *selection* (3-3 classified Merge "U" for that reason,
and rule 2 rules out selection-reading verbs).

**Verbs.** `(subtract a b)`, `(intersect a b)` and `(xor a b)` are indexed
like `merge`, with `OpSpec`s in a new `boolean_specs()`. The bridge registers
all four in one loop (`register_boolean_verbs`). That split was also needed
because the old lifecycle function went over clippy's line limit. A body
against itself is a quiet no-op. `docs/scripting.md` verb reference and the
roadmap M21 line are updated.

**Two pre-existing bugs found and fixed, because this feature hits them on
ordinary input:**
1. *Collider panic.* `Collider::convex_decomposition` panics ("compound
   shape must contain at least one shape") on a fine near-circular ring of
   ~240+ vertices. parry's `from_convex_polyline` reads edges turning by
   < ~0.026 rad as collinear and the hull collapses. A box ∩ contained circle
   crashed the app. `physics::body_sync` now decimates rings to 1e-3 of the
   shape's extent first, with the new `geometry::contours::decimate_ring`
   (Ramer-Douglas-Peucker, unit-tested). This affects colliders only;
   meshes still read the full contour.
2. *Lost arcs in contours.* `contour::simplify_ring` started greedy
   simplification at an arbitrary vertex with nothing kept yet. It compared
   each vertex only with its immediate neighbours, so a curved run at the
   start of a ring was dropped whole. The screenshot showed an intersect's
   lower arc rendered as one straight chord. It now starts from the ring's
   sharpest vertex and measures against the last kept vertex. Unit test:
   `a_curve_at_the_start_of_a_ring_keeps_its_vertices`.

**Tests.**
- `tests/it/booleans.rs` (9):
  - subtract of a circle: an analytic tree checked by containment, tool
    consumed, pins kept/deleted by material, one undo with records equal,
    redo;
  - intersect: containment, the operand's hinge rewired to the same world
    point, internal joint deleted, outside pin deleted, undo equal;
  - xor around a contained circle: one analytic body;
  - xor of overlapping boxes: the split decision; two polygon pieces of
    area 2 at the right centroids, pins follow their pieces, the
    overlap's pin is deleted, undo equal;
  - subtract of a bar: splits;
  - refusals: empty, miss, disjoint xor, swallow, self, single;
  - ground refused;
  - undo labels per op;
  - box ∩ circle builds a collider (the panic regression).
- Plus `scripting::scripts_run_the_body_booleans`, `BooleanIntent` in
  `reflect_intents`, and `Xor` in the persistence proptest.
- Geometry: xor field/bounds/composition, `decimate_ring`, and the
  contour regression.

**Screenshots** (not in git; described in a PR comment):
- a box minus a circle (corner bite with a clean concave arc);
- box ∩ circle (a D-shape with both arcs; the first attempt showed the
  contour bug above);
- the multi-selection context menu with the three new entries under Merge;
- an xor run from that menu, splitting into two pieces (box minus lens, and
  the circle's outer cap).

Screenshot-harness note: egui drops a click held longer than its
`max_click_duration` (0.8 s), so with lavapipe the Pause button needs
`mousedown 1; sleep 0.3; mouseup 1`. The suggested "sleep 1" never pauses.

**Follow-ups (not done):**
- Split pieces lose analytic geometry (polygon leaves), as cut pieces do.
  Keeping a tree per piece would need a component-isolating field.
- Boolean ops with a `SmoothUnion` op through `BooleanIntent` work
  mechanically (labelled `merge`), but 4-2 owns the UX.
- The bounds-miss refusal is AABB-level. A rotated operand whose box
  overlaps the host but whose solid does not still consumes it on subtract
  (the result is just the host).
- Collider decimation also changes polygon-leaf colliders slightly (≤ 1 mm
  per metre). No test moved.
- `docs/state.md` hazard text for screenshot clicks (above) is the
  orchestrator's to fold in.
