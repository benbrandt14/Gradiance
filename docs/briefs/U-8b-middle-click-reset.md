# Every number field resets on middle click
Phase 5 · owner note on review question `number-fields` (2026-11-19,
`docs/review/archive.ron`) · status: in review

Written by the orchestrator, 2026-11-19, at the ingest of six review
answers.

## Goal

The owner kept the number fields as built and added: "Should reset to
default on middle click." Make a middle click reset **every** number
field in the app to its default, with one undo step on the edit seam,
and prove it with a test.

## Facts (orchestrator, 2026-11-19, main `92ab302`)

- `crates/gradiance-ui/src/unit_fields.rs:377` already reads
  `response.clicked_by(egui::PointerButton::Middle)` as a reset, and the
  doc comment at :258 says so. No test covers it, and the review
  question's description did not mention it. *Unverified:* whether it
  works in the app. A middle drag also orbits the view; egui may not
  report a click when the pointer is over a panel, or the canvas may
  take the press.
- 38 sites call `egui::DragValue` directly (U-8 Outcome, "Cut": optimizer
  15, node graph 6, groups 5, widgets 3, array 2, settings 2, signals 2,
  context menu 1, reflect grid u32 1). They have no default and no
  reset.

## Approach

1. Test first: an egui-harness test that middle-clicks a hovered
   `number_field` and expects the default and one undo step. Then the
   same in the running app through `ui.click` with the middle button (add
   the button to `ui.click` if it lacks one), screenshot evidence.
2. Fix whatever step 1 shows (the canvas taking the press, or egui's
   click test).
3. Sweep the raw `egui::DragValue` sites onto `widgets::precise_drag` (edit
   seam) or `widgets::labelled_drag` (config seam) with a default, so the
   reset, Backspace and the context menu reach them. A site with no
   meaningful default keeps egui's plain drag: list it in the Outcome
   with the reason.
4. `docs/ui-design.md`: the behaviours table names middle click.

## Out of scope

A real slider widget (retained in `docs/state.md`), pointer wrap, new
key bindings, column multi-edit.

## Done when

- Middle click resets a field in the test and in the app.
- The raw `DragValue` count is in the Outcome, before and after.
- Gates green (fmt, clippy, doc, boundaries, targeted tests; CI for the
  workspace). Outcome filled.

## Outcome

Branch `claude/project-thread-tw1mvl-u8b`, PR #268, stacked on 5-4
(#266).

### Landed

- **Step 1, test first.** The middle click already worked in the widget:
  `unit_fields::behaviour_tests::a_middle_click_resets_a_field_once`
  (press and release a frame apart, as a mouse does: one commit, the
  default shown; a second click at the default commits nothing) and
  `a_middle_click_without_a_default_changes_nothing` (no default: the
  value stays and no text entry opens). Nothing to fix in egui's click
  test, and the canvas does not take the press: the camera orbits only
  off the UI (`camera.rs`, `!over_ui.0 && orbiting`).
- **In the app** (Facts' "unverified"): a middle click on the inspector's
  friction (0.900 typed) shows 0.500, the default; the view does not
  orbit; one Ctrl+Z gives 0.900 back (one undo step). Driven by the new
  **`ui.click_button`** (`(ui-click-button at button)`, `left` / `middle`
  / `right`; `UiInput::Click` now carries its button).
- **Step 3, the sweep.** Every raw `egui::DragValue` moved onto the
  helpers, with a default:
  - config seam: `widgets::config_field(default, speed)` builds a
    `ConfigField` (`.range`, `.unit` / `.stored`, `.prefix`, `.suffix`,
    `.scientific`; `.show` for `f32`, `.show_int` for any integer), drawn
    through `unit_fields::number_field_as` (a `Format`: prefix, suffix,
    fixed decimals, scientific). `labelled_drag` / `labelled_drag_int`
    take one (`labelled_quantity` and `labelled_drag_u32` folded in).
    Optimizer (weights, gap, contact reach, neighbours, settle-toward,
    steps, boundary sizes, convergence), node graph (binding domain,
    block constants), groups (distribute bounds, seed, range chip),
    signals (binding domain), array (gap, counts), settings (gravity,
    solver, camera, light strength, ambient, shadow reach), context menu
    (plane friction, fillet radius). Defaults: the settings type's
    `Default`, `PackConfig::default()`, `GroupsPanel::default()`,
    `SignalMap::default()`, or the value the picker starts the field at
    (boundary 1 : 1, 4 m, 3 m; 8 steps; array gap from
    `ArraySpacing::ALL`, count 4; a new block's constants 1).
  - **reflect grid**: `reflect_grid*` are generic over `T: Reflect +
    Default` and reset each number (f32, Vec2, u32, quantity newtypes,
    nested structs) to the same field of `T::default()`.
  - edit seam: the tracer node editor (`node_kind_editor`, three hosts)
    now uses `precise_drag_clamped` (new) for fade and size, and reports
    a finished edit only: **one undo step per drag** where it used to
    emit a `PropertyEditIntent` every frame of a drag.
- `number_field` no longer clamps a value to its range by showing it
  (`clamp_existing_to_range(false)`): the tracer size field (range
  0.5–20 m, pixel-era, retained) was rewriting a 0.02 m size to 0.5 m on
  every frame it was drawn.
- `docs/ui-design.md` § 2: the `ConfigField` row, middle click in the
  behaviours table, "no raw `DragValue`"; `docs/scripting.md`,
  `docs/ops.md`, `tools/screenshot/README.md`: `ui.click_button`.

### Decisions (`docs/review/archive.ron`, `decision: true`)

- `number-field-default`: a settings field resets to its type's
  `Default` (for lighting, the studio look's value), not the active
  look's preset.
- `click-button-op`: a new op `ui.click_button` rather than a field on
  `ui.click` (a Lisp call checks its argument count exactly, so every
  `(ui-click at)` would break).

### Measured

- Raw `egui::DragValue` sites outside `unit_fields` (the field's own):
  **38 before, 0 after**. No site was kept for lack of a default.
- Tests: the two field tests above; `widgets::layout_tests::
  a_middle_click_resets_config_fields` (a float and an integer),
  `a_middle_click_resets_a_tracer_field_once`; `reflect_grid::tests::
  a_middle_click_resets_a_grid_number_to_the_type_default` (f32, u32,
  `Length`); `tests/it/ui_input.rs::
  a_named_button_click_presses_that_button`.
- Snapshot `groups_panel.png` regenerated: the distribute and range
  fields now show three decimals (0.500, 5.000), like every field.

### Not checked

- `cargo test --workspace` (CI).
- A middle click with a real mouse that moves a few points between press
  and release (egui still calls it a click below its drag threshold).
- Screenshots of the swept panels (optimizer, node graph, groups): their
  reset is covered by the widget tests, not by a live run.

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p gradiance-ui
--lib` (126); `cargo test -p gradiance-scene --lib review`; `cargo test
--test it` filtered to `op_conformance`, `ui_input`, `ui_panels`,
`ui_groups`, `ui_registry`, `ui_snapshots`, `settings`, `pack`,
`signal`, `review`, `scripting`, `units_display`. All green. rustc
1.100.0 (the pin). Live run on lavapipe under Xvfb (screenshots outside
git).

### For later

- Over a hovered number field **Ctrl+Z does not undo**: U-8's key claim
  (`claim_keys`) takes every key while the pointer is on a field. Found
  in the live run; a candidate for a small brief (let chords with Ctrl
  other than C/V through).
- The tracer size range 0.5–20 m is still pixel-era (retained, 5-1).
