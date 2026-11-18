# Audit follow-ups: world-pin slider, group membership, motor display
Audit · owner request 2026-11-18 (A-1 findings) · status: in review

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

**In review (2026-11-18, worker).** PR #261, stacked on A-1 (#260) → 4b-4
(#259) → U-7 (#258) → 4b-3 (#257). No screenshots (all three fixes are
covered headless).

### Landed

1. **World-pin slider reversed (high, A-1 finding 7): fixed.**
   `JointDef::world_first` now covers sliders, so a world-pin slider is
   derived `(pin, body A)` with the axis carried into the pin's frame at body
   A's rest angle (`physics::joint_sync`). New `JointDef::slider_frame` (the
   travel's origin and direction: body A's anchor body-to-body, the pin on a
   world pin) and `JointDef::slider_travel` (the engine's position) are the
   one place the frame lives; the gizmo (`render::joint_viz`), limit handles,
   handle drag and picking (`interaction::joint_edit`) and the oscillate
   seam (`physics::motor`, which skipped world pins) read them. The length
   sensor is an unsigned distance and did not change. Tests (each compares
   world pin with body-to-body, both failed first on the world pin):
   `joints::a_positive_slider_motor_drives_along_the_axis_on_both_slider_kinds`
   (rotated body), `joints::world_pin_slider_limits_are_the_body_against_the_world`,
   `joints::an_oscillating_slider_motor_reverses_on_both_slider_kinds`; unit
   tests `joint::tests::a_world_pin_slider_measures_the_body_from_the_pin`,
   `joint_edit::tests::world_pin_slider_handles_measure_from_the_pin`.
   `defaults::a_default_slider_motor_reaches_its_speed_at_any_mass` no
   longer flips the sign.
2. **A body in two sibling groups (medium, A-1 finding 8): fixed.**
   `GroupCommand` closes the target set over every group it partly holds
   (`GroupIndex::close_over`: a member brings its whole group, as a viewport
   click selects it) and inserts the new id between the groups it holds and
   the groups that hold it (`GroupIndex::insertion_depth`), so a subset of a
   group nests **inside** it. One undo step, same command seam. Tests:
   `groups::grouping_a_grouped_body_wraps_its_whole_group`,
   `groups::grouping_part_of_a_group_nests_inside_it` (both failed first),
   unit `group::tests::a_new_group_closes_over_the_groups_it_overlaps`.
3. **Motor inspector (low, A-1 finding 12): fixed.** The default hinge
   speed is exactly 20 rpm (`DEFAULT_MOTOR_ANGULAR_VELOCITY` = 2.094 rad/s;
   was 2 rad/s, shown as 19.099 rpm); the rpm field drags in whole rpm; an
   auto torque/force field shows the ceiling the physics seam derived (new
   read `PhysicsQueries::motor_ceiling`), so a drag starts from it and pins
   an explicit cap, and middle-click returns to auto (0). A fresh slider
   motor now keeps its own 2 m/s (it took the hinge default through the
   shared view). Goldens regenerated (`v8_every_quantity.ron`,
   `replay_joints.ron`: one `target_velocity` each). Test:
   `joint_inspector::motor_display_tests::a_fresh_hinge_motor_reads_20_rpm_and_its_derived_torque`.

### Open (recorded, not fixed)

4. **Opening a dock re-centres the view (A-1 finding 13).** The scene camera
   renders into the pane left by the docks (`ui::apply_scene_viewport`), so
   the pane's centre is the camera's centre; with the perspective camera the
   scale also follows the pane height. Proposal (needs a brief: it writes
   the camera from the UI layer's pane change): when the pane rect changes,
   move the camera by the pane-centre shift times the world-per-pixel, and
   scale the camera distance by the old/new pane-height ratio, so every
   world point keeps its window pixel.
5. **Short arms lose motor speed (4b-4).** Measured (headless, default
   motor, arm pinned at one end, frames 30–120, fraction of the target):

   | substeps | 0.1 m slowest / mean | 0.3 m slowest / mean | 1 m slowest / mean |
   |---|---|---|---|
   | 1 | −2.40 / 0.03 | −0.37 / −0.12 | 0.61 / 1.05 |
   | 6 (default) | 0.27 / 0.56 | 0.82 / 1.09 | 0.96 / 1.04 |
   | 12, 24, 48 | same as 6 | same | same |

   An explicit torque cap of 4·L³ or 1000·L³ N·m gives the same numbers, so
   neither the auto ceiling nor the substep count is the cause, and both
   proposed small fixes (more substeps for jointed bodies, a larger
   ceiling) do nothing. The speed oscillates around the target (mean above
   1 for 0.3 m), which points at the point constraint's position
   correction feeding back into the angular velocity after the motor's
   dead-beat step. Proposal (needs a brief): sweep the motor rate below the
   cap (4b-4 measured 120/s → 0.48 / 1.53 against 360/s) and avian's joint
   compliance on short arms, or drive the motor as a velocity target about
   the pivot in a custom constraint.
6. **Field strength does not scale (4b-4 Cut).** Not small. The surface
   acceleration is `strength · field mass / 1 kg`, clamped at 500 m/s²
   (`physics::fields::MAX_FIELD_ACCEL`), and the falloff length is a fixed
   1 m. With the default −2000 m/s²: a 0.1 m pebble pulls 20 m/s² at its
   surface and 5 m/s² 1 m away; a 1 m box pulls the clamp (500 m/s²) out to
   1 m; a 10 m ρ10 source pulls the clamp out to ~62 m. A fix needs the
   falloff length scaled to the source size and a default chosen by feel
   (an interactive question with scenes): a brief, not a tweak in
   `tuning.rs`.

### Decisions

1. World-pin slider positive = the body moves along the drawn axis, limits
   measured from the pin: the `world-pin-sign` rule extended (its archive
   note now says so). Saved world-pin slider motors and asymmetric limits
   run the other way after loading (format may change freely).
2. Grouping closes over partly held groups and nests subsets: archive
   entry `group-overlap` (`decision: true`; alternatives re-parent and
   refuse).
3. Default hinge speed 20 rpm rather than display rounding: the field then
   reads a round number without hiding precision in typed values; 4b-4's
   defaults tests use the constant and still pass.

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`; `-p` domain, physics, interaction,
command, scene, ui (lib); `it` modules joints, joint_sensors, joint_edit,
joint_breaking, defaults, tools, physics_sync, replay, scripting, groups,
ui_groups, editor_ops, op_conformance, reflect_intents, remote, ui_panels,
persistence, ui_conflicts. The full workspace run is CI's. rustc 1.100.0 =
`RUST_PIN`. CI on 1701fba failed clippy on two new test lines (fixed in
8a826e3).
