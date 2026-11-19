# Screenshot audit of operations and menus, with fixes
Audit · owner request 2026-11-18 · status: in review

Written by the worker (owner ruling "scoped work", `docs/state.md`: a
session may write the brief when the work is in the plan; a bug found by a
screenshot gets a brief, as 2-5 and 4-4 did). Stacked on 4b-4 (#259) →
U-7 (#258) → 4b-3 (#257).

The owner's request, verbatim:

> take pictures of all operations and menus and identify errors and resolve
> them. Particularly with tool interactions, motors, selection, slicing, and
> multi bodies with joints.

## Goal

Every tool, every menu and every panel is exercised once in the real app
(lavapipe + Xvfb, driven over `--remote` with the `ui.*` operations) and
photographed. Each defect found is a row in the Findings table below, with
an evidence image; the small ones are fixed in this PR with a test that
failed first.

## Read first

- `tools/screenshot/README.md` (the harness), `docs/scripting.md` § CLI and
  MCP (`gradiance op …` against a `--remote` app), `docs/ops.md`.
- Briefs U-3, U-4 (widget names, `ui.inspect`, input injection), 4b-4's
  Outcome (bugs seen, not fixed).

## Method

1. One build, `tools/screenshot/session.sh start` (`--remote`), driven with
   `session.sh op` / `ui.click`, `ui.drag`, `ui.key` on names from
   `ui.inspect`. Images go to `/mnt/project-files/a-1/` (not git), with a
   README listing each in one line.
2. Priority order: (1) tool interactions, (2) motors, (3) selection,
   (4) slicing/cutting, (5) multiple bodies with joints, then (6) every
   menu and panel once.
3. For each finding: a before image, then a failing test where the bug is
   testable headless, the fix through the existing seams (invariants in
   `CLAUDE.md`), an after image.
4. A fix that needs a design change is recorded as "needs a brief" with a
   one-line proposal. Interactive judgement calls go to
   `docs/review/questions.ron`.

Known items to verify first: world-pin motor sign reversed vs a
body-to-body hinge (retained "world-pin angle sign (2-1)"); `motor_gain`
ignores `SimSettings::speed`; a fast select-tool drag lands a body offset
by half its size; the colour chip on a mixed group selects the whole
group; the `look` review scenes' `colour.scm` paints wrong bodies when
indices are read before spawns land.

## Out of scope

New features, redesigns of a tool or panel, keys (owner ruling), the
retained items that are not bugs.

## Done when

- Every tool, menu and panel has at least one image; the Findings table is
  complete.
- Each fixed finding has a test (where headless-testable) and an after
  image.
- fmt, clippy, doc, boundaries and the touched `-p` / `it` modules green;
  the rest in CI.

## Outcome

**In review (2026-11-18, worker).** Stacked on 4b-4 (#259) → U-7 (#258)
→ 4b-3 (#257). 57 images in `/mnt/project-files/a-1/` (not in git), one
line each in its `README.md`.

### Landed

Six fixes, each with a test that failed first (except the palette, which
was checked in the running app):

1. **Drag tools started one step late** (`gradiance-interaction/src/lib.rs`):
   the PreUpdate chain that reads the button edges and the world cursor was
   unordered against `InputSystems`, so a press reached the tools a frame
   late, after the cursor had moved. Every drag tool started one pointer
   step from the press (a 1.2 × 1.0 m box drag drew 1.08 × 0.90 m), and a
   fast select drag missed the body it was pressed on. This is also the
   retained "the select tool grabs the snap point nearest the drag's first
   moved frame". Now `.after(InputSystems)`. Tests:
   `ui_input::a_fast_box_drag_spans_from_the_press_point`,
   `ui_input::a_fast_select_drag_moves_the_pressed_body`.
2. **World-pin hinge reversed** (`JointDef::world_first`,
   `JointDef::hinge_angle`, `physics::joint_sync`): derived (body, pin), so
   a positive motor speed turned a world-pinned body clockwise while body B
   of a body-to-body hinge turned counter-clockwise; asymmetric limits were
   mirrored against the gizmo's arc; the angle sensor read minus the body's
   turn. Now derived (pin, body). Tests:
   `joints::a_positive_hinge_motor_turns_counter_clockwise_on_both_hinge_kinds`,
   `joints::world_pin_hinge_limits_are_the_body_against_the_world`; the
   4b-4 `defaults` motor rigs no longer flip the sign. Retires the retained
   "world-pin angle sign (2-1)" for hinges.
3. **Motor gain ignored `SimSettings::speed`** (`retune_joints`): avian
   steps the fixed timestep times `Time<Physics>`'s relative speed. Test:
   `joints::the_motor_gain_follows_the_simulation_speed`.
4. **Groups panel chip / nested row selected the outermost group**
   (`SelectTransition::SetExactly`): a filter's result is a query, not a
   viewport click, so it is not group-expanded. Test:
   `ui_groups::a_chip_on_a_mixed_group_selects_only_the_matching_bodies`.
   Retires the retained 4b-2 item (both halves).
5. **The `spring` verb hung b from the wrong point**
   (`command::ops::joint::joint_record`): one world anchor was converted
   into both bodies' frames, so b's anchor was a's centre in b's frame and
   b hung a rest length below a's centre plus their distance. This hit the
   4b-4 `spring-feel` review scenes (all four options); they now hang from
   the centres (image 82). Test: `scripting::retyped_quantities_read_and_write_as_numbers`
   (anchors asserted).
6. **The tool palette covered the bottom dock** (`ui::toolbar`, `ui::lib`):
   drawn before the docks at a fixed height, it hid the Node Graph tab.
   Now drawn after the docks, sized to its content and scrolling in the
   height they leave.

### Findings

| # | Area | Operation / menu | What is wrong | Evidence | Severity | Fixed? |
|---|---|---|---|---|---|---|
| 1 | Tools | every drag tool (box, circle, select, cut…) | gesture starts one pointer step after the press; fast select drag misses the body | 01, 04, 10 → 05 | high | yes (fix 1) |
| 2 | Motors | hinge motor on a world pin | positive speed turns the body clockwise, body-to-body counter-clockwise; limits mirrored vs the arc; sensor sign flipped | 24–26 → 27 | high | yes (fix 2) |
| 3 | Motors | motor gain | ignores `SimSettings::speed` | test only | low | yes (fix 3) |
| 4 | Selection | Groups panel chip / nested row | selects the whole outermost group | 32 → 33 | medium | yes (fix 4) |
| 5 | Joints | `spring` verb (and the `spring-feel` review scenes) | far end anchored at a's centre in b's frame | 51, 52 → 53, 82 | high | yes (fix 5) |
| 6 | Panels | tool palette + bottom dock | palette covers the Node Graph tab | 7x-panel-nodes → 74, 75 | medium | yes (fix 6) |
| 7 | Motors | slider on a world pin | same reversed order as the hinge: body sits on the mirrored side of the drawn travel, motor runs backwards | 15, 16 | high | **needs a brief**: extend `world_first` to sliders (axis into the pin frame, span and limit handles drawn from the pin, oscillate translation from the pin) |
| 8 | Selection | `group` verb / Ctrl+G on a grouped body | a body can sit in two sibling groups; the panel then draws one group nested under the other | 31 | medium | **needs a brief**: grouping a member should group its outermost group (as the viewport selects), or refuse |
| 9 | Tools | Tracer | trail dots drawn as circles metres wide | 15, 17 | medium | no: retained 5-1 (pixel-era tracer sizes) |
| 10 | Panels | joint window | covers the Properties dock | 21 | low | no: retained (UI/render) |
| 11 | Scripting | `--script a.scm --script b.scm` | both read one snapshot, so `look/colour.scm` finds no bodies (review mode, one frame apart, is correct: 81) | 80 | low | no: retained ("two start-up scripts share one snapshot") |
| 12 | Motors | joint inspector | `max torque (auto)` shows `0 N·m` instead of the derived ceiling; default target reads `19.099 rpm` | 22 | low | no: display only; belongs to U-8 number fields |
| 13 | Panels | right/bottom dock | opening a dock re-centres the world on the smaller pane (bodies near the top slide under the transport) | 10, 7x-panel-nodes | low | no: design (camera follows the pane) |
| 14 | Motors | world-pin motor start | the first frames can run backwards before tracking (seen at lavapipe frame rates, not headless) | 26 | low | not reproduced headless; watch |

Verified working (no finding): polygon, circle, ground, Drag tool, corner
scale handle, band select, Ctrl+G, cut through one body, through three, and
through a world-pinned bar (the hinge stays on its piece), undo/redo of
cuts, a 4-link chain, deleting a link (its two hinges go, undo restores
all three), File/Edit/View/Help menus, every panel opened once, the `look`
review question in review mode.

### Decisions

1. World-pin hinge positive = counter-clockwise (the body against the
   world), as body B on a body-to-body hinge: archive entry
   `world-pin-sign`. Saved world-pin motors and asymmetric limits turn the
   other way after loading (format may change freely; no migration).
2. The Groups panel's filter selects exactly its result
   (`SetExactly`), while a viewport click still expands to the outermost
   group. Small, inside scope.
3. The palette scrolls rather than wrapping into two columns when the
   bottom dock leaves too little height: smallest change; no review
   question (no alternative was tried).

### Not checked

- Right-click (the context menu, right-drag rotate) and modifier-held
  pointer gestures (Shift-click, Ctrl-drag array): `ui.click`/`ui.drag`
  inject the left button only and `ui.key` releases its modifiers. Covered
  headless by `tools::` and `array_drag::`.
- Joint breaking in the running app (headless `joint_breaking::` passes).
- Motor oscillation and limits in the running app after fix 2 (headless
  `joints::` covers both).

### Cut

Findings 7 and 8 (need briefs); 9–11 are retained items.

### Checked locally

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D
warnings`, `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`, `cargo test --test boundaries`, `-p` gradiance-domain (with
doctests), -physics, -interaction, -command, -ui; `it` modules ui_input,
joint* (joints, joint_sensors, joint_edit, joint_breaking), defaults,
ui_groups, scripting, review, replay, tools, interaction, ui_panels,
ui_snapshots (lavapipe), persistence, groups, op_conformance,
physics_sync, rod, array_drag, history. The full workspace run is CI's.
rustc 1.100.0 = `RUST_PIN`.
