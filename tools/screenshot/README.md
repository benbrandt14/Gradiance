# Screenshots without a GPU

Agent containers have no GPU and no display. This harness runs the real app on
**lavapipe** (Mesa's CPU Vulkan) under **Xvfb**, and drives it through the
app's own operations over the remote surface (`--remote`): input with
`ui.click`, `ui.drag`, `ui.key`, `ui.text`, `ui.wheel` (brief U-4), frames
with `ui.screenshot`, widget positions with `ui.inspect` (brief U-3). Xvfb
only gives the app a display; nothing is sent through X.

**Ad-hoc screenshots are never committed.** Write them outside the repo (the
default is `/tmp/gradiance-shots`) and attach them to the conversation or the
PR discussion instead. Images a test or a review question uses may be
committed (owner, 2026-11-17).

## Setup (once per container)

```sh
apt-get install -y mesa-vulkan-drivers libvulkan1 xvfb libxkbcommon-x11-0 curl
cargo build --locked --bin gradiance      # honours CARGO_TARGET_DIR
```

`tools/screenshot/session.sh` finds the binary at `<repo>/target/debug/gradiance`;
set `GRAD_BIN` if your `CARGO_TARGET_DIR` is elsewhere, `GRAD_PORT` for
another remote port (default 15702).

## Use

```sh
T=tools/screenshot/session.sh
$T start --script my-scene.scm   # app + Xvfb on :77, 1280x720, --remote; log → $GRAD_SHOT_DIR/session.log
sleep 30                          # lavapipe start-up is slow (or poll `$T op ui.inspect`)
$T op ui.inspect '{"prefix": "tools:"}'          # the palette's buttons and their points
$T op ui.click '{"at": "tools:Box"}'              # a widget by name …
$T op ui.drag '{"from": [540, 300], "to": [700, 400], "steps": 10}'   # … or window points
$T op ui.key '{"key": "Space"}'                   # pause; keys need no window focus
$T grab /tmp/gradiance-shots/drag.png             # ui.screenshot; waits for the file
$T stop
```

`$T op <id> [params-json]` is one `gradiance.op` call; any operation in
`docs/ops.md` works (`body.count`, `edit.undo`, …). A start-up script can
call the same verbs (`(ui-click "tools:Box")`, `(ui-screenshot "a.png")`),
but a widget name needs a drawn frame, so name lookups belong in a later
call, not in the first frame's script.

Things learned the hard way:

- **The simulation plays from start-up** (`GameState::Playing` is the default).
  Press Space (`ui.key`) or click the transport's Pause to freeze a layout;
  undo does nothing while it plays.
- **Coordinates** are egui logical points from the window's top left. In the
  default 2D view, world `(0, 0)` is at `(640, 370)` and 1 m = 100 points, y
  up. Prefer widget names from `ui.inspect` over measured points.
- **A gesture plays over the next frames** (one pointer event per frame) and
  the call returns at once: wait a moment before the next screenshot.
- **The select tool grabs the snap point nearest the drag's first moved
  frame** (a body's centre or an edge midpoint) and puts it on the cursor, so
  a drag with a long first step can land a body offset by half its size.
- **Physics runs slow** at software-rendering frame rates; wait longer than you
  think for motion to settle, and compare two grabs to see whether anything is
  still moving (`compare -metric AE a.png b.png diff.png`, ImageMagick).

## Snapshot tests (no Xvfb)

`tests/it/ui_snapshots.rs` (brief U-5) renders a few egui panels with
`egui_kittest` on a wgpu adapter (lavapipe here, as in CI) and compares them
with the PNGs in `tests/snapshots/ui/`. They need `mesa-vulkan-drivers`
and `libvulkan1`, not Xvfb or the app.

```sh
cargo test --test it ui_snapshots                     # compare
UPDATE_SNAPSHOTS=1 cargo test --test it ui_snapshots  # rewrite after an intended change
```

- **Tolerance:** a pixel counts as changed past a YIQ distance of 0.6
  (`egui_kittest`'s default); a snapshot fails when more than 20 pixels
  change. Changing which tool is highlighted moves 1585 pixels.
- **On failure** `<name>.new.png` and `<name>.diff.png` appear next to the
  committed file (git-ignored); CI uploads them as the `ui-snapshot-diffs`
  artifact.
- **No adapter:** the tests print why and pass, unless
  `GRADIANCE_REQUIRE_GPU=1` (CI sets it), which fails them.
- Committed snapshots are fixed-size (1 pixel per point) and a few KB each;
  review their diff like code.

