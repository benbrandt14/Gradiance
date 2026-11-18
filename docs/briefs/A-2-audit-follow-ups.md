# Audit follow-ups: world-pin slider, group membership, motor display
Audit · owner request 2026-11-18 (A-1 findings) · status: in progress

Written by the worker (owner ruling "scoped work", `docs/state.md`: a bug
found by a screenshot or test in a landed feature gets a brief). Stacked on
A-1 (#260) → 4b-4 (#259) → U-7 (#258) → 4b-3 (#257).

## Goal

The open findings of the A-1 screenshot audit are fixed or recorded, highest
severity first:

1. **(high)** A world-pin slider runs mirrored (A-1 finding 7): the body
   sits on the far side of the drawn travel and a positive motor speed
   drives it backwards. Apply A-1's pin-first rule
   (`JointDef::world_first`, archive entry `world-pin-sign`) to sliders:
   motor direction, limits, the travel the gizmo draws and the oscillate
   seam.
2. **(medium)** Grouping a body that is already in a group puts it in two
   sibling groups (A-1 finding 8). Decide the rule, implement it through
   the existing command seam, test it.
3. **(low)** Joint inspector: `max torque (auto)` shows `0 N·m`; the
   default target reads `19.099 rpm`. Opening a dock re-centres the view
   (A-1 finding 13).
4. **(low)** Short gravity-loaded arms lose motor speed (4b-4 Outcome).
   Investigate; fix only with a small, safe change, else record a proposal.
5. **(low)** Field strength does not scale with the source (4b-4 Cut).
   Fix if small, through `gradiance-domain/src/tuning.rs`, with a
   behaviour test.

## Read first

- `docs/briefs/A-1-visual-audit.md` Outcome (findings 7, 8, 12, 13; fix 2,
  the hinge precedent).
- `docs/briefs/4b-4-scaled-defaults.md` Outcome (Cut: short arms, field
  strength).
- `crates/gradiance-domain/src/joint.rs` (`world_first`, `hinge_angle`),
  `crates/gradiance-physics/src/joint_sync.rs`, `physics/src/motor.rs`,
  `interaction/src/joint_edit.rs`, `render/src/joint_viz.rs`.
- `docs/ui-design.md` § 2 (numeric fields).

## Approach

- Each fix: a test that fails first, the fix through the existing seams
  (`CLAUDE.md` invariants 1–5), then green.
- Interactive judgement calls go to `docs/review/questions.ron` with
  scenes; plan decisions are the worker's, recorded in
  `docs/review/archive.ron` (`decision: true`) or in the Outcome.

## Out of scope

Every other retained item in `docs/state.md`; new features; keys.

## Done when

- Items 1–3 fixed with tests (or, for a judgement call, a review question);
  4 and 5 fixed or recorded with a measurement and a proposal.
- fmt, clippy, doc, boundaries, the touched `-p` crates and `it` modules
  green locally; the rest in CI.

## Outcome

(filled at handoff)
