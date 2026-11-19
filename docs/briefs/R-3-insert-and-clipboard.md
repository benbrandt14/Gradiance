# Insert a scene file and copy/paste bodies as RON text
Phase R · roadmap § "Run & read surface" · status: in review

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

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-r3`.

**Landed (three operations, all in `docs/ops.md`):**

- `crates/gradiance-command/src/insert_cmd.rs`: `InsertIntent` →
  `InsertCommand` (undo label `insert`), `fresh_copy` (new ids for every
  body, joint and node, poses moved by an offset), `fragment` (the part of
  a scene record a copy writes) and `fragment_anchor` (the centre of the
  box around the body positions). The joint and node remaps are
  `spawn::remap_joint_def` / `spawn::remap_node`, extracted from
  duplicate's `clone_internal_joints` / `clone_attached_nodes`, which now
  call them: one rule for duplicate, array, insert and paste. The command
  makes the group ids fresh with duplicate's `remap_clone_groups` (now
  returning its `(old, new)` pairs) and carries the group names over;
  it refuses an id the scene already holds (`CommandError::IdTaken`) and a
  joint or node whose body is not in the fragment.
- `crates/gradiance-persist/src/ops.rs`: `scene.insert` (`insert-scene
  path at`; `""` = a dialog through `InsertSceneRequest`, whose answer is
  invoked as `scene.insert`; File ▸ Insert…), `edit.copy` (`copy-ron
  targets`; editor seam; reads the gated `SceneDump` mirror, writes
  `ClipboardWrite` and returns the text; an empty selection returns `""`
  and writes nothing) and `edit.paste` (`paste-ron text at`). Insert and
  paste mint ids from the context, return the new body ids, and lower to
  one `InsertIntent`: one undo step each. `at`: `[]` keeps positions,
  `[x, y]` centres the bodies there.
- Keys and menus: `EditorAction::Copy` (Ctrl+C, a `KEY_OPS` row →
  `edit.copy` on the selection) and `EditorAction::Paste` (Ctrl+V → a
  `PasteRequest` with the cursor). A hovered number field keeps both
  (`FIELD_ACTIONS`, with Delete; decision `field-ctrl-chords`); a focused
  text field takes every key as before. Edit ▸ Copy / Paste, File ▸
  Insert….
- `crates/gradiance-ui/src/clipboard.rs`: the bridge. `write_clipboard`
  puts a `ClipboardWrite` on bevy_egui's `EguiClipboard` and in
  `LastCopy`; `paste_clipboard` reads `EguiClipboard`, else `LastCopy`,
  and invokes `edit.paste`. Installed headless too. Both run only on a
  frame with their message (`on_message`): **no per-frame cost**.
- Tests: `tests/it/clipboard.rs` (a saved mechanism, two hinges and a
  named nested group, inserted over itself and again at (10, 0): 6 then 9
  bodies, no id used twice, the nesting renamed one-to-one onto unused
  group ids, the name moved, one `insert` step each, both undo whole; a bad
  path, bad text or bad `at` is an error and changes nothing; copy → paste
  through the operations keeps the inside hinge, drops the outside pin, and
  centres at `at`; Ctrl+A, Ctrl+C, Ctrl+V paste at the cursor in one step;
  Ctrl+C / Ctrl+V on a hovered field do nothing to the scene). Unit tests
  for `fresh_copy`, `fragment`, the anchor, `at` and the paste example.
- `BodyRecord::appearance` defaults when absent (`#[serde(default)]`), so
  hand-written scene text for a paste may leave it out (as physics, depth
  and groups already could). What a save writes is unchanged.
- Docs: `docs/scripting.md` (verb list and § "Parts and the clipboard"),
  `docs/ops.md` regenerated.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`insert-placement`, `clipboard-text`, `clipboard-path` (no new crate:
bevy_egui's `manage_clipboard` already links arboard), `insert-ids`.

**Not done / follow-ups:**

- A paste does not select what it added (the selection lives in
  `gradiance-interaction`, above the command that knows the ids; the op
  returns them). A later UI brief can select the returned ids.
- Not checked: the real system clipboard (X11/Wayland) in a window, and
  the File ▸ Insert… dialog. Headless tests use the `LastCopy` fallback.
