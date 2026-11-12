# Screenshots without a GPU

Agent containers have no GPU and no display. This harness runs the real app on
**lavapipe** (Mesa's CPU Vulkan) under **Xvfb**, drives it with `xdotool`, and
grabs frames with `xwd`, so a feature or UI change can be checked visually.

**Screenshots are never committed.** Write them outside the repo (the default
is `/tmp/gradiance-shots`) and attach them to the conversation or the PR
discussion instead.

## Setup (once per container)

```sh
apt-get install -y mesa-vulkan-drivers libvulkan1 x11-apps xdotool imagemagick xvfb
cargo build --locked --bin gradiance      # honours CARGO_TARGET_DIR
```

`tools/screenshot/session.sh` finds the binary at `<repo>/target/debug/gradiance`;
set `GRAD_BIN` if your `CARGO_TARGET_DIR` is elsewhere.

## Use

```sh
T=tools/screenshot/session.sh
$T start --script my-scene.scm   # app + Xvfb on :77, 1280x720; log → $GRAD_SHOT_DIR/session.log
sleep 30                          # lavapipe start-up is slow
$T x mousemove 32 406; $T x click 1          # palette clicks select tools
$T x mousemove 440 560; $T x mousedown 1; $T x mousemove 840 560
$T grab /tmp/gradiance-shots/drag.png        # mid-gesture
$T x mouseup 1
$T stop
```

Things learned the hard way:

- **Keyboard shortcuts do not arrive.** With no window manager the window never
  gets a focus-in event, so key presses are dropped; use the tool palette and
  the menus with clicks instead. Mouse input works.
- **The simulation plays from start-up** (`GameState::Playing` is the default).
  Click the transport's Pause button to freeze a layout.
- **Coordinates:** in the default 2D view, world `(0, 0)` is at screen
  `(640, 370)` and 1 m = 100 px, y up. The tool palette is at x ≈ 32, from
  y ≈ 140 (Select) down in palette order.
- **Physics runs slow** at software-rendering frame rates; wait longer than you
  think for motion to settle, and compare two grabs to see whether anything is
  still moving (`compare -metric AE a.png b.png diff.png`).
