# Editor areas and saved workspaces
Phase U · `docs/unified-surface-plan.md` § 7.3, § 10 row 11, § 11 (dock
layouts persist as workstation config) · status: ready

Written by the orchestrator, 2026-11-19 (owner ruling "scoped work").

## Goal

1. **Areas.** Any dock tile can switch the editor it shows (outliner,
   properties, node graph, console, plot, groups, signals, …) from a small
   type menu in its tab strip, as Blender's editor-type dropdown does. The
   switch goes through the `panel.*` operations, so a script and the remote
   surface can do it too.
2. **Workspaces.** A workspace is a saved dock layout (which editors, where,
   what size). Three ship: **Layout** (today's default), **Scripting**
   (console + outliner large), **Signals** (node graph + plot large). A
   workspace tab row at the top switches between them; the person can save
   the current layout as a new workspace and delete their own.
   Operations: `layout.save`, `layout.apply`, `layout.workspace.select`
   (SolidCraft names, § 7.3), plus list and delete.

Workspaces are **workstation config** (like `GridSettings`): saved with the
person's settings, never scene content, never in undo history (owner ruling
2026-11-17, `state.md` "Earlier architecture decisions may be revisited").

## Read first

- `docs/ui-design.md` (design system, `PanelToggle`, theme tokens, glyphs).
- `crates/gradiance-ui/src/dock.rs`, `dock_sync.rs` (`sync_panes`: keep it;
  no tree rebuild), `panels.rs`, `panel_ops.rs`, `menu.rs`
  (`Panels::named`), the U-3b and U-9 Outcomes.
- `scene::records` `scene_content!` (workstation config is classified
  there; a new resource must be).
- `docs/unified-surface-plan.md` § 7.3 and § 11.

## Facts (orchestrator, 2026-11-19, main after #272)

- The dock is `egui_tiles`; `dock_sync::sync_panes` keeps panes in step with
  the panel resources without rebuilding the tree.
- `panel.*` operations exist (U-3b) and `Panels::named` is the one table
  of panel names (14 panels after U-9).
- *Unverified:* where workstation config is saved today (with the scene
  file, per `CLAUDE.md` invariant 4) and whether a per-user config file
  exists. Report it; if none exists, workspaces save with the scene as
  workstation config, like `GridSettings`, and the Outcome says so.

## Approach

1. A `Workspace` record (name, tile tree as panel names + split fractions)
   in the layer that owns panels; serde; a `Workspaces` resource classified
   as workstation config.
2. Capture and apply a layout through `dock_sync` (apply = set panel
   visibility + tree shape, never a raw tree rebuild that loses state).
3. Editor-type menu per tile; it swaps the tile's panel through
   `panel.*`.
4. Workspace tab row (theme tokens; the active one in the accent).
5. Operations above, registered in the owning layer; `docs/ops.md`
   regenerated.
6. Tests: apply a workspace and read back visible panels and tile order;
   save → apply round trip; switching does not touch the undo stack or
   scene content; a script switches a tile's editor. Screenshots of the
   three workspaces on lavapipe.

## Out of scope

Adjust Last Operation (U-11), key bindings for workspaces, floating
windows, node-canvas positions (wait for G-0), the Joint window overlap
(retained).

## Done when

- The three workspaces switch in the app and a saved one round-trips
  (screenshot evidence).
- Tests pass; fmt, clippy, doc, boundaries green locally; CI green.
- Outcome filled; `docs/state.md` updated on the U-10 lines.

## Outcome

*(worker fills in)*
