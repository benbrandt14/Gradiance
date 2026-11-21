# Editor fixes: New keeps the workstation, mirror rows, chain strokes
Phase Q (follow-ups) · 6-1 Outcome "Noticed, not changed" (File ▸ New),
N-9 Outcome (no context-menu mirror rows), N-5 Outcome (a long Adjust Last
entry) · status: after Q-2

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

Three small editor defects, one PR:

1. **File ▸ New keeps the person's workstation.** Today `scene.new`
   loads the default environment, workstation config included, so it
   resets the person's grid, snap, camera feel and saved dock layouts, and
   undo does not bring them back (6-1 Outcome). After this brief, New
   replaces the bodies, joints, nodes and the scene-content settings, and
   leaves grid, snap, camera and workspaces as they were. File ▸ Open is
   unchanged (a file carries its own workstation config, by design of the
   split).
2. **Mirror in the context menu.** The body context menu gets "Mirror
   horizontally" and "Mirror vertically" rows that invoke `body.mirror_h`
   and `body.mirror_v` on the selection, from the same table as the Edit
   menu.
3. **A drawn chain records a thinned path.** The chain tool commits every
   stroke point (4 px apart), so a long stroke makes a long Adjust Last
   entry and a long `body.chain` call. After this brief the committed path
   is thinned (RDP within 1 screen pixel), as the sketch tool does; the
   laid links do not change by more than that pixel.

## Read first

1. `CLAUDE.md` invariant 4 (scene content against workstation config,
   the `scene_content!` list).
2. `crates/gradiance-command/src/ops/scene.rs:72-108` (`NewScene`,
   `empty_scene`) and `crates/gradiance-command/src/scene_cmd.rs`.
3. `crates/gradiance-ui/src/menu.rs:280-310` (`MIRROR_ROWS`) and
   `crates/gradiance-ui/src/context_menu.rs:316` (`context_menu`).
4. `crates/gradiance-interaction/src/tools/chain_tool.rs` and
   `crates/gradiance-interaction/src/tools/sketch_tool.rs:110-120` (the
   thinning to copy).

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- `NewScene::lower` sends `LoadSceneIntent { scene: empty_scene() }`
  (`crates/gradiance-command/src/ops/scene.rs:90-94`); `empty_scene`
  uses `EnvironmentRecord::default()` (`:99-108`).
- `LoadSceneCommand::apply` validates shapes and calls
  `SceneRecord::apply` (`crates/gradiance-command/src/scene_cmd.rs:26-32`),
  which installs every environment field
  (`crates/gradiance-scene/src/records.rs:400`).
  `EnvironmentRecord::apply_scene_content` (`records.rs:444`) installs only
  the scene-content half.
- The workstation fields: `grid`, `snap`, `camera`, `workspaces`
  (`records.rs:469-497`); the scene-content list is `records.rs:458-467`.
- The 6-1 undo matrix leaves the workstation config out of its compare
  (`NOT_UNDONE` in `tests/it/undo_matrix.rs`); its `scene.new` row must
  still pass.
- `LoadSceneIntent { scene }` (`crates/gradiance-command/src/intent.rs:248`)
  is also sent by File ▸ Open and `scene.load`.
- `MIRROR_ROWS` (`crates/gradiance-ui/src/menu.rs:304`) feeds the Edit
  menu; each row writes `InvokeOp::with(id, {"targets": ["selection"]})`
  (`menu.rs:294-296`).
- The chain draft records a point every `SAMPLE_PX = 4.0`
  (`chain_tool.rs:30`) and commits `stroke_to(cursor)` as is
  (`chain_tool.rs:82-86`). The sketch tool thins with
  `rdp(&stroke, THIN_PX * ctx.cam_scale)` (`sketch_tool.rs:114`); `rdp`
  is `crates/gradiance-geometry/src/sketch.rs:91`.

## Approach

1. Add a flag to the load, for example `LoadSceneIntent { scene,
   keep_workstation: bool }` and the same on `LoadSceneCommand`: with the
   flag, `apply` installs the records and calls `apply_scene_content`
   instead of the full environment apply. `scene.new` sets it; Open,
   `scene.load`, review mode and the loaders do not. Undo and redo stay
   whole-scene (the stack snapshot), and the workstation config is
   untouched by both, as for any other step.
2. Context menu: a "Mirror" pair under the body section, read from
   `MIRROR_ROWS` (move the table where both menus can read it), each an
   `InvokeOp` by id. No key (owner ruling).
3. Chain: thin the committed stroke with `rdp` at `THIN_PX *
   ctx.cam_scale` before `build_chain`, keep its first and last point.
   The preview may keep the raw stroke.

Record the New choice as the `new-scene-keeps-workstation` decision (done
by the planning thread in `archive.ron`; the worker changes
`ingested_as` to the code path).

## Tests

- `tests/it/history.rs` (or a new module): set a grid spacing, a snap
  option and a saved workspace, run `scene.new`: the bodies are gone, the
  scene-content settings are the defaults, the three workstation values are
  unchanged; undo brings the bodies back and still leaves the workstation
  values alone. File ▸ Open of a file with another grid still loads the
  file's grid.
- The context menu shows both rows for a body selection (`ui.inspect`
  widget names or a unit test in `context_menu.rs`) and a click
  records one `mirror` step.
- A drawn chain's recorded `body.chain` path holds far fewer points than
  the stroke for a long straight drag (2 points for a straight one), and
  the links lie within one pixel of the unthinned result.
- The 6-1 undo matrix still passes. fmt, clippy, doc, `--test
  boundaries`, the touched `-p` crates and `--test it` modules.

## Out of scope

- File ▸ Open, Insert and Paste behaviour.
- The same-band hit-test order (J-1 "Not checked"): bodies in one band
  have no draw order to follow (their faces z-fight), so there is nothing
  to match yet.
- Keys for mirror.

## Outcome

To be filled by the worker at handoff.
