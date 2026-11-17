# In-app input injection by widget name
Phase U · `docs/unified-surface-plan.md` § 10 row 6 ("**U-4** in-app input
injection") and § 8 tier T2, input half · status: in review

Written by the worker (owner ruling "scoped work", `docs/state.md`: a
session may write the brief when the work is in the plan).

## Goal

A caller outside the window clicks, drags, types and scrolls in the app by
widget name or by window point: `ui.click`, `ui.drag`, `ui.key`, `ui.text`
and `ui.wheel` are `gradiance-op` operations owned by `gradiance-ui`, so
Lisp, the remote surface and later the CLI/MCP (U-6) get them with no code
of their own. A name resolves through the `WidgetSnapshot` mirror (U-3) to
its click point. The input enters **inside** the app as Bevy input
messages, so egui and the interaction tools see it as real input, and the
screenshot harness no longer needs `xdotool` (keys arrive without window
focus; no 0.8 s click hold; no hand-measured coordinates).

## Read first

- `docs/unified-surface-plan.md` § 6 (porting rules, Craft protocol ids),
  § 8 (tiers; "injection writes Bevy input messages, not `ButtonInput`").
- `docs/briefs/U-3-widget-registry-inspect-screenshot.md` Outcome (names
  `<area>:<label>`, `WidgetSnapshot`, `ui::host`).
- `crates/gradiance-ui/src/ops.rs`, `host.rs` (the layer's operations and
  host adapter).
- `crates/gradiance-interaction/src/pointer.rs`, `cursor.rs` (what the
  tools read: `ButtonInput<MouseButton>` edges, `Window::cursor_position`).
- bevy_egui 0.43.1 `src/input.rs` (what egui reads).

## Facts (worker, 2026-11-17, measured on `97abe8f`)

- **egui reads pointer input from `WindowEvent`**, not from `CursorMoved`
  / `MouseButtonInput` (`write_pointer_moved_and_button_messages_system`);
  keys from `KeyboardInput`, text from a pressed `KeyboardInput`'s
  `logical_key` (`Key::Character`), wheel from `MouseWheel`, all mapped to
  a context by the message's `window`. bevy_winit writes each event twice:
  the specific message and the `WindowEvent` (`forward_bevy_events`). Its
  input systems run in `PreUpdate` after `InputSystems`.
- **The tools read the cursor from `Window::cursor_position`**
  (`cursor.rs:24`), which only winit sets (`set_physical_cursor_position`);
  a `CursorMoved` message alone moves nothing. Writing the window's cursor
  makes bevy_winit warp the OS pointer (`changed_windows`), as `xdotool`
  did.
- **Headless** (the integration tests): no `WindowPlugin` (so no
  `CursorMoved`/`WindowEvent` message types), no window, no camera, no egui
  pass. `InputPlugin` is present: `KeyboardInput` and `MouseButtonInput`
  reach `ButtonInput`, leafwing and `PointerButtons` headless.
- Operation params have fixed arity (the Lisp positional mapping counts
  declared fields); no operation uses `serde(default)`.
- **Upstreams:** `tangency-labs/vectorcraft` and `tangency-labs/craft-libs`
  do not answer `git ls-remote` without credentials (private or absent).
  SolidCraft (`a39b74e`) has the Craft injection: `docs/control-protocol.md`
  (`ui.click/drag/key/text/scroll` params) and `crates/ui-egui/src/control.rs`
  + `lib.rs` `synthetic_input` (rules: one pointer event per frame; a drag
  positions a frame before it presses; `steps` 1–200, default 10; non-finite
  coordinates are refused before anything is queued). It injects egui
  events into eframe, so the rules port and the file does not.
- Not checked: whether VectorCraft's rules differ from SolidCraft's.

## Approach

1. `ui::host` gains the injection: a `UiInput` message (one gesture,
   resolved to window points) and an `InputQueue` that expands a gesture
   into frames (pure, unit tested) and plays one frame per update in
   `PreUpdate` before `InputSystems`, writing what winit writes: the
   specific message **and** the `WindowEvent`, plus the window's cursor
   position. Keys use the primary window's entity (a placeholder headless).
2. `ui::ops`: five operations, `Seam::Editor`, no undo step. A target
   (`at`) is a widget name or `[x, y]`; a name resolves in `lower` through
   the `WidgetSnapshot` mirror (handle, else rect centre); an unknown name
   is an error.
3. `ui.key` reads the chord text the registry's `chord` field uses
   (`Ctrl+Shift+Z`, `Delete`); a test parses every declared chord.
4. `tools/screenshot/session.sh` and README: drive by operation (start-up
   script or remote), drop `xdotool` and `xwd`; keep Xvfb.

Invariants: no new mutation path (input is what a person would type; the
tools still commit one command per gesture); egui stays in `gradiance-ui`;
no new crate edge.

## Out of scope

- Snapshot tests on lavapipe (U-5); `panel.*` operations (U-3b); CLI/MCP
  (U-6); keymap changes (owner: keys out of scope).
- Naming more widgets than U-3 did.
- An "input done" signal (a gesture plays over the next frames).

## Done when

- `tests/it/ui_input.rs`: a click on a published palette name plays as
  move, press, release on the target point; an injected `Ctrl+Z` reaches
  the editor (undo); an injected drag on a body ends as exactly one undo
  step; every declared chord parses; unknown names and non-finite points
  are refused.
- Unit tests for the frame expansion and the chord parser.
- Conformance tests pass; `docs/ops.md` regenerated; `docs/scripting.md`
  rows; `session.sh`/README updated; `vendor/UPSTREAM.toml` /
  `docs/upstream-watch.md` record the porting decision.
- fmt, clippy, doc, `tests/boundaries.rs` local. Outcome filled.

## Outcome

**Landed (worker, 2026-11-17), PR in review.** Five operations join the
`ui` namespace (`docs/ops.md`: 69 operations, was 64), all `Seam::Editor`,
no undo step of their own:

| Id | Lisp | Params |
|---|---|---|
| `ui.click` | `(ui-click at)` | `at`: widget name or `[x, y]` |
| `ui.drag` | `(ui-drag from to steps)` | two targets, `steps` 1–200 |
| `ui.key` | `(ui-key chord)` | `"Ctrl+Z"`, `"Delete"`, `"F1"`, `"A"` |
| `ui.text` | `(ui-text text)` | text; `\n` presses Enter |
| `ui.wheel` | `(ui-wheel at lines)` | target, wheel lines (> 0 up) |

Code: `crates/gradiance-ui/src/host/input.rs` (new: `UiInput`, the frame
expansion, `parse_chord`, `InputQueue`, `play_input`), `ops.rs` (the five
operations, `Target`), `host.rs` (module line). Tests:
`tests/it/ui_input.rs` (5), 6 unit tests in `host::input`.

**Porting decision: rules ported, no file.** VectorCraft and craft-libs are
not reachable (`git ls-remote` asks for credentials). SolidCraft
(`a39b74e`) has Craft's injection, but it pushes egui events into eframe,
so its rules port and its code does not (as U-3 did for the screenshot
settle rule): one pointer event per frame; a drag positions a frame before
it presses; `steps` 1–200; non-finite points refused before queueing. Cited
in `host::input`'s module docs; `docs/upstream-watch.md` records it. Nothing
added to `vendor/UPSTREAM.toml` (it lists ported files only).

**Decisions (non-interactive, owner ruling 2026-11-17):**

1. *What is written:* per event, the specific Bevy message **and** its
   `WindowEvent`, as bevy_winit does (egui reads the pointer only from
   `WindowEvent`), plus `Window::set_cursor_position` for a move (the tools
   read the cursor there; bevy_winit then warps the OS pointer, as
   `xdotool` did). Played in `PreUpdate` before `InputSystems`. Never
   `ButtonInput`.
2. *Left button only, fixed arity.* Lisp's positional mapping counts
   declared fields, so optional `button`/modifier fields (Craft's `button?`,
   `shift?`, `hold?`, `via?`, `double?`) would make every call pass them.
   Right/middle clicks and modifier-held pointer gestures are left for a
   brief that needs them.
3. *`at` is untagged:* a string is a widget name, a pair is a point.
   `ui.wheel` keeps the plan's name (Craft calls it `ui.scroll`).
4. *A name resolves at lower time* through the `WidgetSnapshot` mirror:
   the handle, else the rect's centre. Unknown name: an error, nothing
   queued.
5. *Keys:* chord text as the registry's `chord` field writes it (a test
   parses every declared chord); modifiers down a frame before the key and
   up a frame after, so leafwing and egui both see them held.
6. *Text* plays in one frame as keys with no physical key
   (`KeyCode::Unidentified`) and a `Key::Character`, so it fills a focused
   field and trips no letter shortcut; `\n` is Enter.
7. *Budget:* the queue holds at most 2000 frames (about 33 s); a gesture
   that would pass it is dropped with a warning (the call cannot fail after
   it returned). Gestures play in order.
8. *Headless:* the operations register and write their messages; keys and
   buttons reach `ButtonInput` (so `ui.key` works in tests), `CursorMoved`
   and `WindowEvent` are added when `WindowPlugin` is absent.

**Finding (not a bug, documented in the README):** the select tool grabs the
snap point nearest the drag's **first moved frame** and puts it on the
cursor. A 10-step drag of 100 points from a 40-wide box's centre grabbed the
edge midpoint (first step at 10, equidistant) and committed the box at 85,
not 100; the test drags in 20 steps. A rest-before-release was tried and
removed: the gesture had no lag to cover.

**Live check (lavapipe, Xvfb, 1280x720, `session.sh` without `xdotool`):**
`ui.click "tools:Box"` selected the Box tool (palette highlight);
`ui.drag` in the scene drew a box (`body.count` 0 → 1); `ui.key "Space"`
paused (transport shows Play); `ui.key "Ctrl+Z"` undid a box while paused
(1 → 0; while playing undo does nothing, by design); `ui.click` on the
console's input, `ui.text "(spawn-box 0 1 0.4 0.4)"`, `ui.click` on Run
spawned the box (field showed the text; keys were captured by egui while
it had focus); `ui.wheel` zoomed in. Screenshots ad hoc, not committed.

**Checked locally** (rustc 1.100.0): `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`cargo test --test boundaries` (8); `cargo test -p gradiance-ui --lib`
(87); `cargo test -p gradiance --test it -- op_conformance remote
scripting ui_ script_examples interaction tools`.

**Not checked:** the full `cargo test --workspace` (CI); headless egui (no
egui pass without the render plugin, so the palette-click → tool and
text → field paths are live-checked only; U-5's lavapipe tests can pin
them); HiDPI scale factors other than 1 (points are logical, the cursor is
set in logical points); a real pointer moving during injection (both
write the cursor; the last write wins); VectorCraft's rules.

**Deviations from the brief:** none in scope. The README's harness hazards
for keys, click holds and hand-measured coordinates are gone; Xvfb stays.
