# Mirror and joint load colour
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Two small items:

1. **Mirror H / V** for the selection: an operation and an Edit-menu row
   over the existing negative scale (no key: hotkeys are not settled).
2. **Joint load colour:** the joint glyph tints from calm to hot by
   `joint-force / break_force` (or by force relative to body weight when
   the joint cannot break), so a person sees which joint will fail.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `body.scale` already says a negative factor mirrors
  (`command/ops/body.rs` ≈ line 131); joints and anchors must mirror too.
- `joint-force` is a catalog quantity; `JointCommon::break_force` exists.
  Glyphs draw in `render::joint_viz`. The tint comes from the scene look's
  colours (one colour source rule).

## Out of scope

Stress on bodies (no stress model).

## Done when

- Mirroring a hinged pair twice gives back the original scene record
  (test); a joint at 90 % of its break force draws in the hot colour.
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-n9`.

### Landed

- **Mirror** (`command::ops::body`): `body.mirror_h` (`(mirror-h targets)`,
  Edit ▸ Mirror horizontally) and `body.mirror_v` (`(mirror-v targets)`,
  Edit ▸ Mirror vertically). Each lowers to one `ScaleIntent` with a −1
  factor in world axes about the middle of the bodies' world bounds
  (half-planes left out): one undo step, labelled `mirror`. No key. The
  menu rows invoke the operations by id on the selection (`InvokeOp`).
  Adjust Last records the call; there is nothing to adjust (targets only).
- **The negative-scale path** (`geometry::scale::BodyScale`,
  `mirror_shape`; `command::scale_cmd`): one negative factor turns each
  body to its reflected rotation (`2·frame − rot`) and reflects its shape
  across one local axis, structurally, so a rotated box stays a box, a CSG
  body stays a tree, and mirroring twice gives the scene record back bit for
  bit (dyadic test scene). A bare half-plane mirrored top to bottom turns
  half a turn instead (its solid side cannot flip in place). The scale
  handle dragged past its pivot takes the same path and its step reads
  Mirror too.
- **Joints and nodes follow every scale** (they did not before: a scaled
  hinged body left its anchors at the old local points): anchors, slider
  axes, node offsets and thruster/laser axes go through the body map; a
  world pin of a scaled body moves with it; rest rotations turn with their
  body; a hinge whose bodies all mirror (a world pin counts as its body's)
  gets `[−max, −min]` limits and a reversed motor. Attached nodes' derived
  positions are recomputed in the same command, so the recorded step
  already shows them in place.
- **Joint load colour** (`render::joint_viz`): `JointLoad`, a derived
  component required by `JointDef` (never saved, never in undo), written
  each frame by `measure_joint_loads` from `physics::queries::joint_force`
  against the break force or, unbreakable, the default rating
  (`domain::joint::load_fraction`: ten times the moving bodies' weight).
  The glyph face blends from the look's `joint` ink (a spring's
  `alternate`) to its `danger` ink, fully hot from `HOT_LOAD` = 90 %
  (`load_colour`, over the new `Rgba::lerp`). World pins keep their
  warning hue as an outer danger ring. Runs headless too (tests read it).
- Five `decision: true` entries in `archive.ron`: `mirror-operation`,
  `mirror-rotated-bodies`, `mirror-joints`, `joint-load-rating`,
  `joint-load-colours`. `docs/ops.md` regenerated (145 operations); verb
  rows and the mirror paragraph in `docs/scripting.md`.

### Cost

`measure_joint_loads`: `O(joints)` per frame, allocation-free: per joint
one `JointForces` read, at most two id lookups, and the `RigidBody` kind
and `ComputedMass` of each connected body; `set_if_neq` writes only on
change. Drawing adds one blend per glyph. Mirror is a one-shot command:
`O(bodies + joints + nodes)`, plus one `polygonize` per body for the pivot.

### Tests

`tests/it/mirror.rs` (5): a hinged pair with a limited motored hinge and a
thruster mirrored H and V: one step labelled `mirror`, anchors still meet,
limits `[−0.5, 0.25]`, motor reversed, thruster offset and axis mirrored,
and mirroring twice gives the bodies, joints and node offset back exactly,
then two undos restore; a rotated pair stays two boxes at the reflected
angle with its hinge intact; a mirrored pinned body carries its world pin
and turns its limits over; mirror is the last operation and an empty
selection records nothing; a pin rated at its weight / 0.9 reads 0.9 load
and draws in the hot colour, an unbreakable hanging pin reads 0.1, and the
saved scene has no load. Unit tests: `mirror_shape` twice is exact and is
the reflected field; `BodyScale` agrees with the world scale for points and
directions over mirrors, scales and half turns; a pure mirror keeps a
rotated box; `load_fraction`.

### Checked locally

fmt, clippy (`--workspace --all-targets -D warnings`), doc (`-D
warnings`), `--test boundaries`; `-p gradiance-geometry -p
gradiance-domain -p gradiance-command -p gradiance-render -p
gradiance-scene` (with doctests), `-p gradiance-ui --lib`; `--test it`:
`mirror op_conformance adjust_last history joints joint_breaking tools
persistence replay review ui_conflicts ui_surfaces scripting
script_examples registry_validation remote ui_registry physics_sync
interaction thrusters lasers nodes`, and `ui_snapshots` with
`GRADIANCE_REQUIRE_GPU=1` (no snapshot changed). Screenshots on lavapipe
(not committed): a pin carrying a heavy body below a light one draws a red
face, a pin holding one body a white one, both with the red pin ring; Mirror
vertically on the three bodies keeps every hinge on its bodies.

### Not done / follow-ups

- No context-menu rows for mirror (the brief asks for the Edit menu).
- The load is linear force only, like breaking; torque is not shown
  (torque-based breaking is retained, 2-2).
- A free node (no body) is not mirrored: the operations name bodies.

