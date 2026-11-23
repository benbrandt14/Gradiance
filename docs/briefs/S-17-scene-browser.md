# Scene browser with thumbnails
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("file browser with thumbnails") · status: ready

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

A Scenes panel shows the scene files in a folder as a grid of thumbnails
with names. Click opens, F2 renames, Delete moves to trash after a
confirm. Save writes a thumbnail next to the scene.

## Read first

- `crates/gradiance-persist` — save/load and the `rfd` dialogs.
- `crates/gradiance-ui/src/panels.rs` — `PanelToggle`.
- The `ui.screenshot` operation — the capture path.

## Facts

- The thumbnail is a sidecar PNG (`scene.ron` → `scene.png`), not inside the
  RON (single-format rule). Capture the viewport only, 256 px wide.
- The folder is workstation config (`GridSettings` class), not scene content.

## Approach

- `persist::thumbnail` on save; a Scenes panel; operations
  `scene.browse_dir`, `scene.rename`, `scene.trash`.

## Out of scope

Online browsing (S-18). Tags, search.

## Done when

- Test: save writes the sidecar; the panel lists it. Gates green.

## Outcome

Done 2026-11-23 (PR on the S-16 branch).

- `command::thumbnail::draw`: 256 x 160 sRGB pixels of a `SceneRecord`,
  bodies in their fill colours on the look's backdrop, far bodies first,
  sampled through `sdf::eval`. Drawn from the records, not captured
  (decision `thumbnail-from-records`).
- `persist::browser`: a save writes the sidecar `a.png` beside `a.ron`
  (a failed thumbnail never fails the save). `list_scenes`, `rename` and
  `trash` do the disk work; `ScenesFolder` is the folder, not saved in the
  scene (decision `scene-browser-folder-not-saved`). Trash moves both
  files into `.trash/` beside them, numbering a second copy (decision
  `scene-trash-folder`).
- Operations `scene.browse_dir`, `scene.rename`, `scene.trash` (Editor
  seam, no undo step) queue a `SceneFileRequest` for the persist system.
- `ui::scenes`: the Scenes panel (View menu, `(panel-show "scenes")`): a
  grid of thumbnails; click opens (`scene.load`), F2 renames, Delete asks
  and trashes. A hovered tile claims the keys, so Delete does not reach
  the canvas.
- Tests: `tests/it/scene_browser.rs` (save writes a 256 x 160 PNG, the
  panel lists it, rename and trash move both files); unit tests in
  `persist::browser`, `command::thumbnail`, `ui::scenes`.
- Screenshot: `/mnt/project-files/screenshots/s17-scenes-panel.png`.

