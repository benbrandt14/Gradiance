# Every number field resets on middle click
Phase 5 · owner note on review question `number-fields` (2026-11-19,
`docs/review/archive.ron`) · status: ready

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

*(worker fills in)*
