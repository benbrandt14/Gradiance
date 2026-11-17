# Widget registry, `ui.inspect` and `ui.screenshot`
Phase U · `docs/unified-surface-plan.md` § 10 row 6 ("**U-3** widget
registry, `ui.inspect`, `ui.screenshot`") · status: in review

Written by the worker (owner ruling "scoped work", `docs/state.md`: a
session may write the brief when the work is in the plan).

## Goal

A panel names its widgets while it draws, and a caller outside the UI finds
them by name: `ui.inspect` returns the widgets the last frame drew (name,
area, rect, the point to click). `ui.screenshot` saves a PNG of the whole
window once the view has settled. Both are `gradiance-op` operations owned
by `gradiance-ui`, so Lisp (`ui-inspect`, `ui-screenshot`), the remote
surface and later the CLI/MCP (U-6) get them with no code of their own.
This is tier T2's read half (plan § 8); U-4 adds input by widget name.

## Read first

- `docs/unified-surface-plan.md` § 6 (porting rules, Craft vocabulary
  `publish_rect` / `handle_at`), § 8 (tiers), § 13.5 (porting files appear
  with their first user).
- `crates/gradiance-op/src/registry.rs` and
  `crates/gradiance-interaction/src/ops.rs` (a layer's `install`, mirror
  filler and operation declarations).
- `crates/gradiance-ui/src/lib.rs`: `GradianceUiPlugin::build` (returns
  early without the render plugin) and the `EguiPrimaryContextPass` chain.
- `tests/it/op_conformance.rs`: every registered operation is driven from
  its example through all doorways, headless.
- `docs/ui-design.md` (no glyph or numeric-field change here).

## Facts (worker, 2026-11-17, measured on `2e06f74`)

- `GradianceUiPlugin::build` returns before installing anything when
  `RenderPlugin` is absent (lib.rs:202). The integration tests are headless,
  so operations must register before that return or the conformance tests
  and the docs generator never see them.
- No operation is declared in `gradiance-ui` yet; `gradiance-ui` has no
  `serde` dependency (an `Operation` needs `Deserialize`).
- Four places already push an area's rect into `PanelRects` each frame
  (menu.rs:112, dock.rs:378, bottom_dock.rs:241, view_cube.rs:167); the
  toolbar's two windows and the palette's buttons publish nothing.
- The 14 systems of the egui pass are one `.chain()`.
- **Upstream is reachable:** `https://github.com/tangency-labs/solidcraft`
  (public, MIT OR Apache-2.0 except `LICENSE-BRAND`, which covers
  `ui-kit`, `look`, `theme` and branding only). At `0f5a5e2`:
  `crates/ui-registry/src/lib.rs` (195 lines, depends on `egui` only:
  `publish_handle`, `publish_target`, `publish_rect`, `handle_at`,
  `rect_of`, `handles`, `rects`, `clear_handles`, in thread-locals) fits
  as a file. `crates/control-egui/src/screenshot.rs` asks egui for the frame
  (`ctx.request_screenshot`) and answers on a channel: its **settle rule**
  ports (150 ms and 3 frames; a moving view waits 500 ms more), the file
  does not (Bevy takes the window from the swap chain, scene included).
- Not checked: craft-libs and VectorCraft repositories (not located or not
  searched; `docs/upstream-watch.md`).

## Approach

1. Port `ui-registry` under `crates/gradiance-ui/src/vendor/solidcraft/`
   in upstream shape (rustfmt and pedantic lints off for the file), and
   create `vendor/UPSTREAM.toml`, `tools/upstream-diff.sh`,
   `docs/upstream-watch.md` with it (§ 13.5: with their first user).
2. A thin adapter `ui::widget_registry`: re-export the publishing API;
   clear at the start of the egui pass, copy the frame's names into a
   `WidgetSnapshot` resource at its end. Upstream's thread-locals need one
   thread for the whole pass: run `EguiPrimaryContextPass` on the
   single-threaded executor (it is one chain already).
3. First users: the six areas publish their rects; each palette tool
   publishes a target `tools:<name>`.
4. `ui::ops`: `ui.inspect` (query, reads the `WidgetSnapshot` mirror) and
   `ui.screenshot` (editor seam, no undo; lowers to a `ScreenshotRequest`
   message), registered before the headless return.
5. `ui::host`: the screenshot queue with the settle rule (pure, unit
   tested) and the Bevy capture (`Screenshot::primary_window` +
   `save_to_disk`).

Invariants: no new mutation path (an operation writes one message through
`OpOutput`); egui stays in `gradiance-ui`; no new crate edge.

## Out of scope

- Input injection by name or point (`ui.click/drag/key/...`): U-4.
- Snapshot tests on lavapipe: U-5.
- `panel.*` operations replacing the four panel VM built-ins, and the
  inspector `PropertyEdit` rows of `SIDE_DOOR_ALLOW` (U-2b's Outcome named
  both "U-3's" work list). **Cut** from this brief to keep it one worker's
  size; see Outcome.
- Naming every widget in every panel: only the first users above.
- A completion signal for a screenshot (the file appears later).

## Done when

- `tests/it/ui_registry.rs`: published widgets reach `ui.inspect` over
  remote (`id` and `name` forms) and Lisp; the palette publishes a target
  per tool in a real egui pass; `ui.screenshot` refuses a non-PNG path,
  returns the path and queues one request (headless: no capture).
- Unit tests for the adapter's snapshot and the settle rule.
- Conformance tests pass with the two new operations; `docs/ops.md`
  regenerated; `docs/scripting.md` has the verb rows.
- fmt, clippy, doc, `tests/boundaries.rs` local; CI green. Outcome filled.

## Outcome

**Landed (worker, 2026-11-17), PR in review.** Two operations in a new
`ui` namespace (`docs/ops.md`: 64 operations, was 62):

| Id | Lisp | Seam | Returns |
|---|---|---|---|
| `ui.inspect` | `(ui-inspect prefix)` | query | array of `{name, area, rect: [x0,y0,x1,y1] \| null, at: [x,y] \| null}` |
| `ui.screenshot` | `(ui-screenshot path)` | editor, no undo | the path; the PNG follows |

**Porting decision: ported.** SolidCraft was reachable over plain git;
`crates/ui-registry/src/lib.rs` at `0f5a5e2` is vendored as
`crates/gradiance-ui/src/vendor/solidcraft/ui_registry.rs` with **one**
local change (`use bevy_egui::egui;`, recorded in `vendor/UPSTREAM.toml`).
`vendor/UPSTREAM.toml`, `tools/upstream-diff.sh` (tried: lists entries and
diffs against upstream HEAD) and `docs/upstream-watch.md` are created with
it. The screenshot settle rule is ported as a rule in `ui::host` (cited),
not as a file. `ui::host` exists now with the screenshot as its first user;
U-4's input injection joins it.

**Decisions (non-interactive, owner ruling 2026-11-17):**

1. *Names:* an area's rect under its own name (`menu`, `transport`,
   `tools`, `right_dock`, `bottom_dock`, `view_cube`), a widget as
   `<area>:<label>` (`tools:Box`); `area` is the text before the first `:`.
   Upstream's own names use the same `prefix:` form (`row:Bodies`).
   Coordinates are egui logical points from the window's top left. Measured
   live (lavapipe, 1280x720): `tools:Select` at `[32, 141]`, matching the
   screenshot README's hand-measured palette position.
2. *One thread per egui pass:* `EguiPrimaryContextPass` runs on
   `SingleThreadedExecutor`. The pass was already one chain, so no
   parallelism is lost; the alternative (changing upstream's thread-locals)
   would break the port's shape.
3. *The mirror is a resource* (`WidgetSnapshot`), filled at the end of the
   pass; the operation context reads that, never a thread-local, so a
   doorway on any thread sees the last finished frame.
4. *Lisp sees each row as JSON text:* the bridge renders a JSON object as a
   string (`json_to_steel`). Changing that is a bridge-wide behaviour change,
   not U-3's; remote (the primary consumer, U-4/U-6) gets real objects.
5. *`ui.screenshot` is `Seam::Editor`, `undo: None`,* path must end in
   `.png` (case-insensitive), no folder allow-list (the remote surface is
   loopback-only and already unauthenticated; an allow-list belongs to U-6's
   MCP, as upstream has one). It returns the path at once: a BRP method
   answers in the same frame, so it cannot wait for the GPU readback.
6. *Settle rule:* upstream's (≥150 ms and ≥3 frames; a camera glide waits
   ≤500 ms more, then takes the view as it is) **plus one Bevy rule**: while
   the renderer is busy (`RenderBusy`: not drawn yet, or any pipeline
   waiting in `PipelineCache`) the request waits and its frame count
   restarts, up to 20 s. **Found by the live check:** without it a
   start-up script's `(ui-screenshot …)` saved an all-black 1280x720 PNG;
   with it the same script saves the drawn window. A running simulation does
   not count as moving (it is the content).
7. *Headless:* both operations register; `ui.inspect` answers `[]`;
   `ui.screenshot` queues and the request is dropped with a warning (no
   window), so the conformance tests drive it like any editor operation.
8. *Dependency:* `serde` added to `gradiance-ui` (workspace pin; needed for
   `Deserialize` on the param structs). Lockfile diff: one line (the
   `gradiance-ui` dependency list); no new package.

**Cut, for a follow-up brief (proposed "U-3b panel ops", before 4b-2):**
register `panel.show/hide/toggle/open` in `gradiance-ui`, delete the four
`BUILTINS` rows, `register_panel_verbs` and the script side of
`PanelRequests`; then the remote Scheme run serves only meaningless
built-ins and can go. The inspector `PropertyEdit` side-door rows stay
U-2b's list. Stale "in brief U-3" texts now say "a later brief".

**Tests:** `tests/it/ui_registry.rs` (3: inspect over remote `id`/`name`
and Lisp; palette targets under `egui_kittest`; screenshot refusal,
return, queue, no capture headless); unit tests in `ui::widget_registry`
(3), `ui::host` (4), and the vendored file's own 6.

**Checked locally** (rustc 1.100.0): `cargo fmt --all -- --check`;
`cargo clippy --workspace --all-targets -- -D warnings`;
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`;
`cargo test --test boundaries` (8); `cargo test -p gradiance-ui --lib`
(81); `cargo test -p gradiance-script --lib` (43);
`cargo test -p gradiance --test it -- op_conformance remote scripting ui_`
(96). Live, on lavapipe under Xvfb: `ui.inspect` over HTTP BRP returned 17
rows; `ui-screenshot` from a start-up script and over remote each wrote a
correct 1280x720 PNG (ad-hoc, not committed).

**Not checked:** the full `cargo test --workspace` (CI); a screenshot
during a camera glide (the 500 ms rule is unit-tested only); multiple
windows (only the primary window is taken); the 20 s render wait on a
renderer that never settles.
