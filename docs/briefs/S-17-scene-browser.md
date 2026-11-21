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
