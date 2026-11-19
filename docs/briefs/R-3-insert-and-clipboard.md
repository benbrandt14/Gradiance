# Insert a scene file and copy/paste bodies as RON text
Phase R · roadmap § "Run & read surface" · status: planned (after R-2)

Written by the planning thread, 2026-11-19. Docs only.

## Goal

- `scene.insert path`: add the bodies, joints and groups of a scene file
  to the current scene with fresh `StableId`s, one undo step. A folder of
  `.ron` files becomes a parts library with no new format.
- Ctrl+C / Ctrl+V on the canvas: copy the selection to the system
  clipboard as the same RON text; paste adds it at the cursor, one undo
  step. A mechanism can then go through chat or an MCP client.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- Duplicate and array already clone bodies with a `StableId` remap and
  internal joints (`command::array_cmd`, `CopyPlacement`). Insert and
  paste reuse that remap; they add no new cloning logic.
- No body copy/paste exists on `main` (no `Paste` outside the number
  fields). Ctrl+C / Ctrl+V on a hovered number field belong to U-8; a
  hovered field keeps those keys.
- bevy_egui sends Ctrl+V as `Event::Text(clipboard)` and has
  `Context::copy_text` (U-8 brief, Facts). No new dependency.
- Roadmap audit row 13 lists clipboard; no brief scheduled it before.

## Out of scope

Paste of settings or signal graphs; a parts-library panel (a later UI
brief can list a folder over `scene.insert`).

## Done when

- Insert and paste are one undo step each, keep internal joints and
  nested groups, and never reuse an existing `StableId` (test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
