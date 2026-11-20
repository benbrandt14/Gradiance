# Joint stress test: overlaps, depth, background, cut/delete/undo
Audit · owner request 2026-11-20 · status: in review

The owner's request, verbatim:

> Create a thread to stress test the joint interactions. Make sure (via mcp
> and lisp verbs) that various groups of joints and anchors and struts
> clicked between multiple bodies (with various degrees of overlap or layer
> movements, or clicked onto background) have correct and intuitive
> behavior. If desired behavior is unknown, default to algodoo or create a
> question for it. Nothing should ever blow up when simulating, no weird
> collisions should occur. All multibody rotability criterion should be
> maintained. Use screenshots to help debug.

Written by the worker, 2026-11-20, in the plan's audit slot (like A-1 to
A-3). Runs beside phase N on its own branch.

## Goal

Every joint kind authored over overlapping bodies, across depth bands, on
the background and through cut / delete / undo is simulated and checked
for: no non-finite pose, no explosion, no hinge pulling apart, no contact
between two bodies that share a non-colliding joint, and hinged bodies
that still turn. Each defect gets a test that fails first, and a fix
through the existing seams.

## Method

`tests/it/joint_stress.rs`: scenes are a few lines of Lisp (the verbs the
tools emit), or tool clicks with an injected cursor. `Watch` plays the run
and records the worst speed, the worst hinge gap, non-finite poses and
contacts between jointed pairs. Live checks ran the real app on lavapipe
(`tools/screenshot/`) over the remote surface; images are in the project
files under `joint-stress/` (not in git).

## Findings

| # | Area | What | Status |
|---|---|---|---|
| 1 | Slider | A body-to-body slider whose body B ran far along the axis exploded: a cart pulled off the end of a rail pinned at its centre reached 6869 m/s at about 4 m of travel, the rail left its pin by 5.7 m. 6, 12 and 24 substeps all failed. Cause: avian's prismatic applies its sideways correction at the two anchors, so body A took it at its own anchor with the wrong lever arm. | **fixed**: `physics::joint_sync::track_slider_anchors` (test `a_cart_run_far_along_a_pinned_rail_stays_on_it`; limits kept: `slider_limits_still_bound_the_travel_on_a_pinned_rail`) |
| 2 | Slider | A world-pin slider rotation-locks its body; a body already turning (a contact knock in the frames before, or the lock pressed during a run) kept turning on its guide forever: avian's `LockedAxes` stops new angular impulses only. | **fixed**: `physics::body_sync::stop_locked_spin` (test `a_spinning_body_stops_turning_when_a_world_slider_locks_it`) |
| 3 | Tools | A hinge on an object-snapped point (a vertex or an edge of the top body, inside the body under it) often made a world pin on one body: exact containment of a point on the boundary is a coin flip. | **fixed**: `ToolWorld::joint_bodies_at`, 1 px tolerance, grounds last (test `a_hinge_on_a_snapped_corner_inside_another_body_joins_both`) |
| 4 | Tools | A spring dragged from empty space to a body made nothing; the reverse drag made a world-pinned spring. Algodoo pins either end. | **fixed** (test `a_spring_dragged_from_the_background_to_a_body_pins_its_start`) |
| 5 | Tools | A hinge click on a ground half-plane alone pinned the static ground to the world: a joint that does nothing. | **fixed**: no joint (test `a_hinge_click_on_the_ground_alone_does_nothing`) |
| 6 | Overlap | Two fully overlapping bars, hinged and pinned; three overlapping bars with a hinge on two; a third free body falling through a hinged pair. | no finding: calm, the hinged bar turns |
| 7 | Rotation | Three bars on one pivot (A to the world, A–B, B–C), a cart with two wheels hinged at their centres on a slope, a closed four-bar loop. | no finding: every hinge turns, the cart rolls; A and C (no shared joint) touch, as in Algodoo (decision `shared-pivot-collisions`) |
| 8 | Depth | A hinged pair on different depth bands; both moved to other layers mid-run. | no finding |
| 9 | Cut / delete / undo | Delete the middle link of a pinned chain mid-run (its two hinges go), undo, redo, undo, play; cut through a hinged pair mid-run, play, undo the cut, play. | no finding |
| 10 | Chain | A chain pinned to the world at both ends. | no finding |
| 11 | Background | A hinge click on empty space makes nothing. | no finding (as Algodoo) |

## Outcome

**In review (2026-11-20, worker).**

### Landed

1. `physics::joint_sync::track_slider_anchors` (`FixedUpdate`, before each
   step): a body-to-body slider's engine anchor on body A slides along the
   axis under body B's anchor; the engine limits shift by the same amount.
   Same constraint, true lever arm. Authored state untouched; world-pin
   sliders are skipped (their first body is the static pin).
2. `physics::body_sync::stop_locked_spin`: a rotation lock that is added
   zeroes the body's angular velocity.
3. `ToolWorld::joint_bodies_at` (1 px tolerance, grounds last) for the
   hinge, slider and spring tools; a hinge on a bare ground makes nothing;
   a spring from empty space pins its start.
4. `tests/it/joint_stress.rs`: 19 scenes. Six of them failed on the code
   before the fixes.

### Decisions

Six `decision: true` entries in `docs/review/archive.ron`:
`joint-anchor-on-edge`, `hinge-on-bare-ground`, `spring-from-background`,
`slider-engine-anchor`, `locked-spin`, `shared-pivot-collisions`. No
interactive question was needed.

### Not checked

- `cargo test --workspace` (CI).
- When two overlapping bodies sit on the same depth band, "topmost" falls
  back to the entity index (`tools::bodies_at_sorted`), not to the draw
  order. A hinge over three such bodies joins whichever two come first.
  Not changed; a candidate for a later brief with the draw order.
- Shift-click and Ctrl-drag joint gestures (the injection holds no
  modifier during a pointer gesture).
