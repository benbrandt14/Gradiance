# Typed length and angle during a draft
Phase C · roadmap § "Unified plan" · status: ready (after C-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 6). Docs only.

## Goal

While a creation draft has a length or an angle under the pointer (box
width/height, circle radius, rod/spring length, polygon or path segment),
typing a number fixes it: `1.5` sets the length, `Tab` moves to the next
quantity (angle, height), `Enter` commits, `Esc` cancels typing. Units and
maths work (`25 cm`, `90 deg`, `2*pi`). The typed value shows in a small
field by the pointer.

## Read first

- `docs/interaction-feel.md` — rows **CAD** and **Blender** (modal typed values); invariants 2-3.
- `crates/gradiance-interaction/src/tools/context.rs:345` — `DraftTool`, the generic driver.
- `crates/gradiance-units/src/expr.rs` — the one parser.

## Facts

- No typed entry during gestures exists today. Number keys may be bound
  (`interaction::input::KEY_OPS`); typing must take the keyboard only while
  a draft is live and the person has started typing a digit, `.` or `-`.

## Approach

- Feel: **CAD/Blender**. The driver owns a `DraftEntry` (text, active
  quantity); a `DraftTool` declares its quantities (`fn quantities()`), and
  the driver overrides the pointer-derived value before `update`.
- The field is drawn by `gradiance-ui` from a published mirror; parsing is
  `gradiance_units::expr`.

## Out of scope

Typed values in the select tool's move/rotate (a later Blender-style G/R/S
brief if wanted).

## Done when

- Pure test of the entry state machine (digits, Tab, Backspace, Esc).
- Behaviour test: box tool, type `2 Tab 50cm Enter` gives a 2 m × 0.5 m box
  in one undo step.
- Gates green.

## Outcome
