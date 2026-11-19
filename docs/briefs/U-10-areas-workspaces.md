# Editor areas and saved workspaces
Phase U · `docs/unified-surface-plan.md` § 7.3, § 10 row 11, § 11 (dock
layouts persist as workstation config) · status: in review

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

Worker, 2026-11-19, PR #275 (stacked on 4b-6, #274).

**Landed.**
- `gradiance-domain::workspace` (new): `Workspace` (name, right and bottom
  `DockLayout`), the tile tree stored flat (`LayoutTile`, index 0 the root;
  no recursive type for reflection or RON) and read through the recursive
  `LayoutNode` (bad indices, cycles and deep trees read as `None`), and
  the `Workspaces` resource (saved list + active name).
- `scene::records`: `workspaces` is an `EnvironmentRecord` field outside
  `scene_content!`, so it is workstation config: saved with the scene,
  never undone. Defaulted when absent; `FORMAT_VERSION` stays 8; the v8
  golden fixture regenerated (one new block).
- `gradiance-ui::workspaces` (new): the presets **Layout**, **Scripting**,
  **Signals**; operations `layout.save`, `layout.apply`,
  `layout.workspace.select`, `layout.workspace.list`,
  `layout.workspace.delete` and `panel.swap` (Lisp `layout-save`,
  `layout-apply`, `workspace-select`, `workspaces`, `workspace-delete`,
  `panel-swap`; editor seam, no undo step); a `WorkspaceMirror` in the op
  context; `apply_layout_requests` and `sync_docks` in `Update`; the
  workspace tab row (`workspace_tabs`, in the top bar after Help: the active
  tab selected in the accent, `+` with a name field saves, a saved tab's
  context menu deletes).
- `dock_sync`: `tree_from_layout`, `layout_from_tree`, `swap_pane`,
  `pane_order` (with unit tests). `sync_panes` is unchanged and still the
  only path that adds or removes a pane between workspace applies.
- Docks: `sync_docks` moved the open-set sync out of `right_dock` and
  `bottom_dock` into `Update`, so the trees exist headless and a test reads
  tile order. `RightDock`/`BottomDock` (now installed headless) keep their
  drawn size and take a workspace's size for one frame (`exact_size`).
  `Pane::name`/`from_name` and `BottomPane::name`/`from_name` map panes to
  `Panels::named` names.
- Areas: each right-dock tab strip has an editor-type dropdown
  (`glyph::TRIANGLE_DOWN`, new; widget names `right_dock:editor:<shown>`,
  `right_dock:pick:<editor>`) that invokes `panel.swap`.
- Docs: `docs/ops.md` regenerated; `docs/scripting.md` (verb rows, a
  paragraph in "Panels are registered ops"); `docs/ui-design.md` § 3
  "Workspaces and areas"; four `decision: true` entries in
  `docs/review/archive.ron` (`workspace-storage`, `workspace-presets`,
  `workspace-tab-row`, `area-editor-menu`).

**Decisions** (each in `archive.ron`).
- No per-user config file exists (checked: no config-dir code anywhere;
  grid, snap and camera travel in the scene file). Workspaces save with the
  scene as workstation config. Loading a scene does not re-apply its
  active workspace.
- Layout = Outliner over Properties (start-up has every dock closed, so a
  literal "today's default" would only close things). No workspace is
  applied at start-up, so start-up chrome and snapshots are unchanged.
- A workspace sets the seven dock editors only; floating windows and the
  status bar keep their state (tested).
- `layout.workspace.select` = make active + apply the saved layout;
  `layout.apply` = re-apply the active one (revert drift). No per-tab
  unsaved state (Blender keeps it; not needed for the brief).
- The editor-type menu is on right-dock tiles only: the bottom dock has one
  editor, and one pane type for both docks is a larger refactor.
  `panel.swap` refuses a switch across docks; exchanging with an editor
  shown elsewhere closes nothing; if `from` is not shown, `to` just opens.
- Applying a workspace builds the dock tree from the layout: that is the
  arrangement being chosen. Pane content lives in resources, so no editor
  state is lost.

**Tests.** `tests/it/ui_workspaces.rs` (5): each preset's visible editors
and tile order; floating windows untouched; save → select another →
select back round trip, the RON round trip through `SceneRecord`,
`layout-apply` after drift, delete falls back to Layout, a preset refused;
no undo step and no scene-content change across select/save/swap/delete;
a script switches a tile's editor (in place, exchange, across docks
refused). Unit tests in `domain::workspace` (4), `dock_sync` (3),
`ui::workspaces` (4). The op-conformance fixture saves a workspace
"Mine" so `layout.workspace.delete`'s example resolves. The glyph-sheet
snapshot regenerated for the new glyph.

**Checked locally (rustc 1.100.0), all exit 0:** `cargo fmt --all --
--check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`cargo test --test boundaries`; `cargo test -p gradiance-ui --lib`;
`cargo test -p gradiance-scene`; `cargo test -p gradiance-domain --lib
workspace`; `cargo test --test it` filtered to `ui_workspaces`,
`op_conformance`, `registry_validation`, `scripting`, `remote`,
`persistence`, `settings`, `history`, `review`, and `ui_` with
`GRADIANCE_REQUIRE_GPU=1` (lavapipe; snapshots included).

**Measured (lavapipe, 1280x720).** The three presets switch from the tab
row; the editor dropdown switched Signals' Plot tile to Script in place;
`+` → "Mine" → Save added a fourth tab; Layout then Mine restored the
saved docks (Script over Properties, node graph below) and sizes.

**Not checked.** `cargo test --workspace` (CI); a scene saved and loaded
through the File menu with workspaces (the RON round trip is tested
headless); the right-click delete on a tab by hand (the operation is
tested); a real mouse resize after a workspace size is applied.

**For later.**
- In Signals (bottom dock 340 points) the tool palette overlaps the node
  graph's tab strip: the palette does not shrink to the height a tall
  bottom dock leaves (any user resize to that height shows it; A-1 fixed
  only the default height).
- One pane type for both docks, so the node graph can switch into a right
  tile and the outliner into the bottom.
- Per-workspace unsaved layout (Blender keeps each tab's live state).
- A per-user config file for all workstation config (grid, snap, camera,
  workspaces), if the owner wants them to stop travelling with scenes.
