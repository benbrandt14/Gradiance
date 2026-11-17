# In-app input injection by widget name
Phase U · `docs/unified-surface-plan.md` § 10 row 6 ("**U-4** in-app input
injection") and § 8 tier T2, input half · status: in work

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

(filled at handoff)
