# Number fields that behave like Blender's
Phase 5 · `docs/unified-surface-plan.md` § 7.2 (the widget-behaviour
table) and § 10 row 10 ("U-8 shares 5-3's parser") · stacked on 5-3
(PR #263) · status: in progress

Written by the worker, 2026-11-19 (owner ruling "scoped work": a session
may write the brief when the work is in the plan).

## Goal

Every number field in the editor behaves the way a Blender user expects:
drag to scrub, Shift for 1/10 speed, Esc during a drag puts the value back
and commits nothing; Ctrl snaps to a sensible step (whole units, 10° for
angles, a step from the range); a typed entry that does not parse says why;
over a hovered field Backspace resets to the default, `-` negates,
Ctrl+wheel steps, Ctrl+C / Ctrl+V copy and paste; right-click offers Copy,
Paste and Reset. An edit-seam field stays exactly one undo step per gesture.

## Read first

- `crates/gradiance-ui/src/unit_fields.rs` (`drag_value`, `stored_drag`,
  the headless field tests) and `widgets.rs` (`precise_drag_in`,
  `labelled_drag`, `labelled_quantity`).
- `crates/gradiance-units/src/display.rs` (`DisplayUnit`, `parse`).
- `docs/briefs/5-3-units-input.md` Revision and Outcome (what it left here).
- `docs/ui-design.md` § 2 (seams: `precise_drag` vs `labelled_drag`).
- egui 0.36.2 `widgets/drag_value.rs` (`impl Widget for DragValue`).

## Facts (worker, 2026-11-19, on `59bf48a`)

- Every quantity field goes through one function,
  `unit_fields::drag_value` (`precise_drag_in` widgets.rs:80,
  `stored_drag` → `labelled_quantity` widgets.rs:211 and the reflect grid
  reflect_grid.rs:93/99, settings.rs:585). `labelled_drag` (widgets.rs
  ≈195) and `labelled_drag_u32` call `egui::DragValue` directly.
- egui's `DragValue` already scrubs at 1/10 speed while **Shift alone** is
  held (`is_slow_speed = shift_only && is_being_dragged`), and ArrowUp /
  ArrowDown step by `speed` while the text is focused. Nothing for Esc
  during a drag, Ctrl, the wheel, Backspace or a context menu.
- `DragValue::update_while_editing` defaults to **true**: every keystroke
  that parses writes the value. With `precise_drag_in` that means typing
  `25` then pressing Esc commits 25 (the field loses focus with the half-
  typed value applied). egui itself skips the parse on Esc only when
  `update_while_editing` is false.
- A failed parse returns `None` to egui, which keeps the old value
  silently; `ExprError` carries a message (`unknown unit `furlongs``)
  that no one shows.
- `KeyboardCaptured` (gradiance-ui lib.rs `capture_pointer_over_ui`) is
  `egui_wants_keyboard_input()` only, and the app's keys read it
  (interaction `input.rs`: Backspace = delete selection, Escape =
  deselect, Ctrl+C / Ctrl+V copy and paste bodies). Hovering a field does
  not capture the keyboard, so Backspace over a field would also delete
  the selection.
- bevy_egui 0.43 sends Ctrl+C as `Event::Copy` and Ctrl+V as
  `Event::Text(clipboard)` (manage_clipboard, a default feature); egui
  0.36 has `Context::copy_text` but no way for a widget to read the
  system clipboard.
- Pointer wrap: bevy_egui computes the pointer delta from positions, so a
  warp of the OS cursor (`Window::set_cursor_position`, used only by
  `host::input` to replay injected moves) arrives as a jump of the drag.
  A wrap needs the host to swallow that jump: a new host seam, not a small
  one.

## Approach

1. **Pure step rule** in `gradiance-units` (`step.rs`): `snap_step(unit,
   shown_speed, shown_range)` and `snap`, unit-tested.
2. **One field** (`unit_fields::number_field`) carries every behaviour;
   `drag_value` stays its thin form. It reports an *instant* edit (wheel,
   Backspace, `-`, paste, Reset) with the value before it, and whether Esc
   cancelled a drag. `precise_drag_in` commits an instant edit at once
   (one undo step) and a cancelled drag never. `labelled_drag` goes
   through it as a dimensionless field, so config rows get it too.
3. `update_while_editing(false)`: a typed entry applies on Enter / focus
   loss, Esc while typing restores.
4. A field that is hovered or dragged claims the keyboard for the next
   frame (`unit_fields::claim_keys`), folded into `KeyboardCaptured`.
5. A bad entry: the field outlines in the theme's error colour and shows
   the parser's message under it for a few seconds.

## Out of scope

Pointer hide-and-wrap (Facts: needs a host seam; cut unless one is small),
drag down a column of fields (needs `op.batch`), collapsible-panel
Ctrl+click, F3 search, pie menus, the status bar (U-9), Adjust Last
Operation (U-11), the keymap (owner: keys are out of scope; Backspace and
`-` act only on a hovered field, never as global keys).

## Done when

- `gradiance-units` tests for the step rule; headless egui tests for Esc
  (no commit), Ctrl snap, Backspace, `-`, Ctrl+wheel, the menu's Copy /
  Paste / Reset, the error hint, Esc while typing; each edit-seam case one
  commit or none.
- Gates green; `docs/ui-design.md` lists the behaviours; Outcome filled.

## Outcome

(filled at handoff)
