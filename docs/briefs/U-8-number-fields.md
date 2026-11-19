# Number fields that behave like Blender's
Phase 5 · `docs/unified-surface-plan.md` § 7.2 (the widget-behaviour
table) and § 10 row 10 ("U-8 shares 5-3's parser") · stacked on 5-3
(PR #263) · status: in review

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

Branch `claude/project-thread-tw1mvl-u8`, stacked on 5-3 (#263) → #262 →
#261 → #260 → #259 → #258 → #257.

### Landed

- **Step rule** `gradiance_units::step` (pure): `snap_step(unit, speed,
  range)` in the shown unit (angle → 10°; finite range → a tenth of its
  decade; else the decade of 10 × speed, never below one whole unit for a
  field with a unit), `fine` (a tenth, Ctrl+Shift), `snap`, `decade`.
- **One field**, `ui::unit_fields::number_field` (`drag_value` is its
  plain form), returning `Field { response, instant, cancelled }`:
  1. Drag to scrub; Shift = 1/10 (egui's own, now tested); **Esc during a
     drag** restores the start value and the edit seam commits nothing.
  2. **Ctrl snaps** while dragging (Ctrl+Shift finer).
  3. Typed maths with units (5-3's parser) now applies on Enter / focus
     loss only (`update_while_editing(false)`); **Esc while typing keeps
     the old value** (it committed the half-typed value before, Facts). A
     **refused entry** outlines the field in the theme's error colour and
     shows the parser's message (`3 furlongs: unknown unit `furlongs``)
     under it for 4 s.
  4. Over a hovered field: **Backspace** = default, **`-`** = negate,
     **Ctrl+wheel** = one snap step per notch, **Ctrl+C / Ctrl+V** copy /
     paste the value with its unit, middle-click = default (moved here from
     `precise_drag_in`). **Right-click**: Copy, Paste, Reset to default.
- **Edit seam** (`precise_drag_in`): a one-step edit commits at once (one
  undo step each: a wheel notch, a key, a menu row); a drag on release; a
  cancelled drag never. Also fixed: the undo record's old value was taken
  after egui had applied the drag's first frame; it is now the value
  before it.
- **Config seam**: `labelled_drag` and the reflect grid's plain numbers
  (f32, Vec2) go through the field as dimensionless, so settings rows get
  the scrub, snap, Esc, typed maths and copy/paste. They have no default,
  so Backspace / Reset do nothing there.
- **Keys**: a hovered, dragged or focused field claims the keyboard
  (`unit_fields::claim_keys`, folded into `KeyboardCaptured` by
  `capture_pointer_over_ui`), so Backspace over a field does not delete
  the selection, Esc does not deselect, Ctrl+C/V do not copy bodies.
- Review question **`number-fields`** (scene `docs/review/scenes/
  number-fields/`, `expected_bodies` row); `docs/ui-design.md` table.

### Decisions

- **A hovered field takes the keys** (archive `number-field-keys`,
  `decision: true`): Backspace / `-` act only under the pointer, never as
  global keys (owner: keys out of scope).
- **Menu Paste reads the last value a field copied** (in egui memory):
  egui gives a widget no read of the system clipboard. Ctrl+V over a field
  pastes the system clipboard (bevy_egui sends its text).
- **Ctrl+wheel adds a step** (it does not round to the step grid), so an
  odd value keeps its offset, as Blender's.
- **Step feel** (snap sizes, scrub speed) is the review question
  `number-fields`, built as described above meanwhile.

### Measured

- `gradiance-units` `step::tests` (6). `unit_fields::behaviour_tests`
  (14, `egui_kittest`, the edit seam re-seeded from its source each frame
  as the inspector does): drag commits once; Shift ≈ 1/10; Esc in a drag
  commits nothing and shows the start value; Ctrl snaps a length (1.3 →
  1 m) and an angle (≈23° → 20°); Backspace, `-`, three Ctrl+wheel notches
  (three commits), a plain wheel (none); the menu's Copy / Reset / Paste;
  Ctrl+V with `40 cm`; Esc while typing (fails with egui's default
  `update_while_editing`, checked); a refused entry shows its message and
  a good one clears it; keys away from a field do nothing and are not
  claimed. 5-3's three field tests still pass.

### Not checked

- The live app: no screenshots; that Backspace over an inspector field
  leaves the selection (the claim is unit-tested, its fold into
  `KeyboardCaptured` is one line). The U-4 harness's injected keys did not
  reach egui text fields in 5-3.
- The full `cargo test --workspace` (CI).

### Cut

- **Pointer hide and wrap**: bevy_egui derives the drag from cursor
  positions, so a warp is a jump of the value; it needs the host to
  swallow the warp's delta (a new `ui::host` seam). Hiding alone, without
  the wrap, would hide where the drag stops at the screen edge.
- 37 raw `egui::DragValue` sites (optimizer 15, node graph 6, groups 5,
  widgets 3 incl. `labelled_drag_u32` and `node_kind_editor`, array 2,
  settings 2, signals 2, context menu 1, reflect grid's u32 1) keep egui's
  plain drag; moving them onto the helpers is a later sweep.
- A live error hint while typing (egui keeps the edit text private; the
  message shows on Enter).
- Column multi-edit (`op.batch`), Ctrl+click panels, F3, pie menus,
  status bar, Adjust Last Operation (out of scope).

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p gradiance-units`,
`-p gradiance-ui --lib`, `-p gradiance-scene --lib review`; `cargo test
--test it` filtered to `review::`, `ui_snapshots::`, `ui_panels::`,
`ui_registry::`, `settings`, `units_display::` (with
`GRADIANCE_REQUIRE_GPU=1`). All green; no snapshot changed. rustc 1.100.0
(the pin).
