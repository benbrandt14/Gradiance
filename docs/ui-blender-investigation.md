# Investigation: a Blender-like UI, with code from BUI, the Craft apps and Graphite

Status: **investigation, not a decision.** Written 2026-11-16 on branch
`claude/laughing-euler-sh2g1r` from `main` `be2a15a`. No feature code. The
owner rulings in `docs/state.md` still apply: no unbriefed work, one worker at a
time. Section 9 proposes briefs for the orchestrator. Section 10 lists the
questions for review mode (brief 4b-0).

Read with: `docs/ui-shell-decision.md` (why egui), `docs/ui-design.md` (the
current design system), `docs/orchestration.md` § Future intent 1 (the UI will
be bootstrapped from an external project with UI-test and MCP infrastructure).

---

## 0. Recommendation in five lines

1. **Do not link or copy BUI.** It is GPL-3.0-or-later C. Gradiance is
   `MIT OR Apache-2.0`. Use BUI only as a **behaviour specification** for
   Blender's widgets, and write the behaviour again in `gradiance-ui` on egui.
2. **Take code from the Craft family (SolidCraft, VectorCraft, LightCraft,
   `storytold/craft-libs`).** Most of it is `MIT OR Apache-2.0`, it is egui,
   and it already has the three things Gradiance lacks: in-app input
   injection, in-app screenshots and a widget registry for tests by name.
   **Exception:** SolidCraft's `ui-kit`, `look` and `theme` crates are under a
   separate brand licence that forbids use in other programs (§ 1.1).
3. **Vendor by module and downport.** The Craft code needs egui 0.37 and a
   fork of egui. Gradiance has egui 0.35. The newest bevy_egui (0.43.1) still
   uses egui 0.36. A git dependency on the Craft crates is not possible now.
4. **Take ideas, not code, from Graphite.** Its UI is a web frontend. Its
   "widgets as data" layout model and its `on_update`/`on_commit` split are
   the useful parts.
5. **Order:** test and control infrastructure first (it makes every later
   UI brief cheaper and checkable on lavapipe), then the theme and widget
   behaviours, then the area/editor model.

---

## 1. What I examined

| Source | Commit date | Licence | UI stack | Notes |
|---|---|---|---|---|
| [ppmpreetham/BUI](https://github.com/ppmpreetham/BUI) | 2026-11-16 (`1f330c9`) | **GPL-3.0-or-later** | C runtime `kit/` + own software raster / D3D11 / GL3.3; Rust binding `blenderui` (taffy layout) | Re-port of Blender `editors/interface`. Own window hosts (Win32, X11, Cocoa). |
| [bherbruck/solidcraft](https://github.com/bherbruck/solidcraft) | 2026 (shallow clone) | MIT OR Apache-2.0 | egui 0.37, eframe, egui-wgpu (vendored) | Crates `ui-kit` (56 k lines), `ui-registry`, `control`, `control-egui`, `mcp`, `mcp-server`, `viewport-egui`, `theme`, `look`. |
| [storytold/vectorcraft](https://github.com/storytold/vectorcraft) | 2026-11-16 | MIT OR Apache-2.0 | egui 0.37 (fork `storytold/egui` rev `6d518fc`) | `ui-egui` (316 k lines), `mcp` with a generated `ui_catalog.rs`. |
| `storytold/lightcraft` (GitHub) | 2026-11-16 | MIT OR Apache-2.0 | egui 0.37 (same fork), wgpu 30 | `ui-egui` has a `headless` module. |
| [storytold/craft-libs](https://github.com/storytold/craft-libs) | shallow clone | MIT OR Apache-2.0 | egui 0.37 (same fork) | Shared by the Craft apps: `craft-ui` (45 k lines: docking, tabs, panes, theme, focus, IME, **screen testing**), `craft-keymap` (17 k), `craft-control` (3 k, no UI toolkit), `craft-control-protocol`, `craft-mcp`. |
| [GraphiteEditor/Graphite](https://github.com/GraphiteEditor/Graphite) | 2026-11-16 | MIT OR Apache-2.0 | Svelte web frontend over a Rust message bus | Read `editor/src/messages/layout` and `input_mapper`. |

### 1.1 Licence exceptions (measured)

- `solidcraft/LICENSE-BRAND` covers `crates/ui-kit`, `crates/look` and
  `crates/theme` (their `Cargo.toml` says `license-file = "LICENSE"`). § 3a:
  "Using any Brand Material in another program … including linking to the UI
  kit, the look or the theme from software that is not SolidCraft" needs the
  maintainers' written permission. **Do not copy from these three crates.**
  `ui-registry`, `control`, `control-egui`, `mcp`, `mcp-server` and `ui-egui`
  use the workspace licence (`MIT OR Apache-2.0`).
- VectorCraft and LightCraft: only the ArtCraft name, wordmark and logos in
  `docs/brand/` are excluded. `craft-libs` `ui`, `keymap`, `control` and `mcp`
  use the workspace licence (`MIT OR Apache-2.0`).

The user named "lightCraft" and "solidCraft". The search results show a
LightCraft on gitea.com (blocked by the container proxy, 403) and the
`storytold/lightcraft` GitHub repository. I examined the GitHub one. **I
assumed** that is the project meant.

Measured version facts (crates.io, 2026-11-16):

| Crate | Gradiance pin | Newest | egui it needs |
|---|---|---|---|
| `bevy_egui` | `=0.41.0` (Bevy 0.19) | 0.42.0 (Bevy 0.19), 0.43.1 (Bevy 0.20) | 0.35 / 0.36 / 0.36 |
| `egui_tiles` | `=0.16.0` | 0.18.0 | not checked |
| `egui-snarl` | `=0.11.0` | 0.12.0 | not checked |
| Craft crates | — | — | **0.37 + a fork** |

Thus no bevy_egui release can host Craft code unchanged. A bump to
bevy_egui 0.42 (egui 0.36) narrows the gap by one minor version and stays on
Bevy 0.19. I did not check whether `egui_tiles`, `egui-snarl`, `egui_plot` and
`egui_kittest` have egui 0.36 releases.

---

## 2. Gradiance today (the baseline)

From `docs/ui-shell-decision.md`, `docs/ui-design.md` and `crates/gradiance-ui`
(about 12 k lines):

- egui shell with a menu bar, a right dock and a bottom dock on `egui_tiles`,
  a fixed left tool strip of PNG icons, a view cube, a context menu.
- A small design system in `ui::widgets` (`section_header`, `precise_drag`,
  `labelled_drag`, …) and a closed glyph set in `ui::fonts::glyph`.
- A panel registry: `PanelToggle` + `menu::Panels::named`. The View menu and
  the `panel-*` script verbs are loops over the same table.
- An op registry (`gradiance-script/src/registry.rs`) and a BRP control
  surface (`gradiance-script/src/remote.rs`: `gradiance.op`, `gradiance.query`,
  `gradiance.ops` on `127.0.0.1`).
- Tests: `egui_kittest` accessibility-tree tests of leaf renderers
  (`tests/it/ui_panels.rs`, 211 lines); no image snapshots.
- Visual checks: `tools/screenshot/` runs the real app on lavapipe under Xvfb
  and drives it with `xdotool`. Known faults (`docs/state.md`): **keyboard input
  does not arrive** (no window manager, no focus), egui drops clicks held over
  0.8 s, coordinates are hand-measured.
- No theme: the app uses the stock egui dark visuals.

The Blender-style north star is stated in `ui-shell-decision.md`. The
structure (docks, T-panel, node editor, menus) exists. The **feel** (widget
behaviour, theme, operator model, area model) does not.

---

## 3. BUI

### 3.1 What it is

BUI extracts Blender's interface code into a C library. The runtime `kit/`
re-ports `interface_widgets.cc` (drawing) and `interface_handlers.cc`
(interaction) and draws through its own software raster or a D3D11/GL3.3
renderer. It has bindings for Python, Rust (`blenderui`), JS, Dear ImGui, C and
Lua. It owns its window (Win32, X11, Cocoa hosts).

### 3.2 Integration options, and my verdict on each

| Option | What it takes | Verdict |
|---|---|---|
| Link `blenderui` as the shell | Composite BUI's frame and Bevy's wgpu frame, route input between two loops. This is option B of `ui-shell-decision.md` ("weeks of R&D"). BUI has no wgpu backend; its GPU paths are D3D11 and OpenGL. | **Reject.** Also the licence below. |
| Copy C or Rust code from BUI | — | **Reject.** GPL-3.0-or-later. Copying makes the combined binary GPL. The licence change is the owner's decision, not a side effect of a UI brief. |
| Copy Blender theme values (`kit_theme_presets.c`, generated from Blender's `Blender_Dark.xml`) | — | **Do not copy the file.** I am not a lawyer. Colour values as facts may be free to reuse, but the file is GPL. A brief should measure colours from Blender's documentation or define its own palette "in the spirit of" Blender. Owner question Q2. |
| Use BUI's README § 17 and Blender's manual as a **behaviour specification** | Write each behaviour again on egui in `gradiance-ui`. | **Accept.** |

### 3.3 The behaviour specification to take (from BUI README § 17)

Each row is a behaviour, where it goes in Gradiance, and how hard it is.
"Seam" means the write seam from `ui-design.md` § 4.

| Blender behaviour | Gradiance home | Effort | Note |
|---|---|---|---|
| Drag to scrub a number field; the pointer hides and wraps; Shift = 1/10 speed; Esc cancels and restores | `widgets::precise_drag_unit`, `labelled_drag` | hours | Esc-restore must not emit an intent. `precise_drag` already holds `gesture-start`, so restore is one branch. Pointer wrap needs a cursor-grab request to Bevy's window; check `docs/bevy19-notes.md`. |
| Ctrl snaps (whole units, 10° for angles, range-based steps) | same | hours | The step rule is pure: put it in `gradiance-units` next to `Dimension`, test it there. |
| Click to type **maths** (`2*pi`, `90/4`) | same | hours | `gradiance-kernel` already compiles expressions. Reuse it for the field parser; do not add a second evaluator. Combine with units input (brief 5-3), so wait for 5-3. |
| Backspace = reset to default, `-` = negate, Ctrl+Wheel = step, right-click Copy/Paste/Reset | same | hours | `precise_drag` already takes a `default`. |
| Drag down a column of fields, then sideways to change all | widgets + a "multi-field gesture" | 1–2 days | Must be **one** undo step for all fields: one `Commit::Done` per field would give N steps. Needs a batch intent, or the dispatcher's batch. Design question. |
| Panels collapse with animation; Ctrl+click opens one and closes the others; drag the grip to reorder | `section_header` → a collapsible panel helper | hours (collapse), 1 day (reorder) | Reorder order is EditorState (not saved), per `ui-design.md` § 4. |
| Menus: press-drag-release; hover switches menu-bar menus | `menu.rs` | hours | egui's menu bar already switches on hover after the first click. Press-drag-release needs a check. |
| **Pie menus** around the pointer | new `ui::pie` | 1 day | Data comes from the action table (`register-action`) so a script can define one. |
| **Menu search** (F3): word initials, typo tolerance, recent first | new `ui::search` over `OperationCatalog` + `Panels::named` | 1 day | The op registry already has names and docs. The search is pure (put the scorer in a lower crate, test it there). |
| Undo inside a text field while typing | egui text edit | — | egui has its own text undo. Accept as is. |
| Float precision: small values show the decimals they need | widgets | hours | Pure function; test it. |
| Tooltips after 0.5 s | egui `Style::interaction.tooltip_delay` | minutes | |

Blender behaviours that BUI does **not** list, but that the Blender-like goal
needs:

| Blender behaviour | Gradiance home | Effort | Note |
|---|---|---|---|
| **Areas with an editor-type switch** (any area can show any editor) | `dock.rs`, `bottom_dock.rs`, `dock_sync.rs` | 2–3 days | Today the two docks hold fixed pane sets. An "editor type" enum per tile over `Panels::named` gives the Blender model. Keep `sync_panes` (no tree rebuild). |
| **Workspaces** (Layout / Scripting / Signals tabs that switch a saved dock layout) | docks + a layout catalogue | 2 days | Layout is workstation config. `orchestration.md` defers "dock layout persistence" to the next planning stage: this needs owner approval (Q4). SolidCraft's `layout.workspace.select` is the model. |
| **Adjust Last Operation** panel (F9 / bottom-left) | new panel over `CommandStack` top | 2 days | Re-runs the last command with new parameters as **one** undo step (undo + redo-with-params). Needs commands to expose their parameters by reflection. Invariant 1 holds: the panel emits an intent. |
| **Status bar** with the current modal tool's key hints | new bottom strip | hours | Read-only projection of `interaction::input::default_bindings` + the active tool. Removes the "binding nobody finds" problem in `ui-design.md` § 5. |
| **Header per area** (menus that belong to that editor) | dock tab UI | 1 day | |
| **N-panel** in the viewport (toggle with `N`) | right dock or overlay | hours | `N` is now the Tracer tool. Key conflict: Q5. |
| Blender dark theme, Inter font, flat rounded widgets | `ui::fonts`, new `ui::theme` | 1 day | Font change touches the glyph-coverage tests (`every_glyph_renders_in_the_proportional_family`). Inter must cover every glyph in `glyph`, or Hack stays as the fallback. |

---

## 4. The Craft family: what to take

The Craft apps follow one architecture. VectorCraft's `docs/architecture.md`
states it: "Every user-visible action is a `CommandSpec` … Menus, the command
palette, Actions, the control channel, the CLI and MCP all reach the same
registry, so anything a user can do an agent can do." Gradiance already holds
this rule (`ui-shell-decision.md`: "a menu item is a registered op"). Thus the
Craft code fits the existing seams. It does not need a new architecture.

### 4.1 Pieces, ranked by value to Gradiance

| # | Piece | Source | What it does | Gradiance use | Cost to bring in |
|---|---|---|---|---|---|
| 1 | **Widget registry** | `solidcraft/crates/ui-registry/src/lib.rs` (195 lines) | Components publish named handles, rects and counts each frame. Tests and the control channel find widgets by **name**, not by pixel. | Replaces hand-measured coordinates in `tools/screenshot/`. A tool publishes its scale handles (`"handle.scale.ne"`), the palette its buttons. | Small. Pure egui types. Downport is trivial. Put it in `gradiance-ui`. |
| 2 | **In-app input injection** (`ui.click`, `ui.drag`, `ui.key`, `ui.text`, `ui.wheel`, `ui.pointer`) | VectorCraft `docs/control-protocol.md`; `craft-control`; SolidCraft `control-egui` | The app queues synthetic egui events itself, so focus and the window manager do not matter. Inputs are validated and budgeted per frame. | **Fixes the lavapipe keyboard fault.** Bevy side: write `KeyboardInput`/`MouseButtonInput` messages and the egui input. Note the retained hazard "headless `ButtonInput` presses lose `just_pressed`" (3-5): the injection must write the message, not the `ButtonInput` resource. | Medium. The protocol text is reusable; the code is eframe-specific and must be written again for bevy_egui. |
| 3 | **In-app screenshot** (`ui.screenshot`) | `solidcraft/crates/control-egui/src/screenshot.rs` | Waits 3 frames and 150 ms (view settles), then takes the presented frame; 8 s timeout; clear error for a hidden window. | Bevy has its own screenshot path (`Screenshot` entity + `save_to_disk` observer in recent Bevy; **I did not check the 0.19 API**). Take the settle rule and the error rules, not the code. Removes `xwd` from the harness. | Small–medium. |
| 4 | **`ui.inspect` / `ui.menu.list` / `ui.menu.invoke`** | VectorCraft control protocol | One call returns tool, panel state, window size, the menu tree; an agent invokes a menu item by command id. | `Panels::named` and the action table are already data. Expose them as `gradiance.ui.*` BRP methods. | Small, but see § 7 (layer rule). |
| 5 | **Generated UI command catalogue** | `vectorcraft/crates/mcp/src/ui_catalog.rs` ("Generated … by `cargo xtask ui-commands`. Do not edit by hand") | The headless MCP server lists UI-only commands without linking the UI. | Gradiance's `gradiance.ops` already lists the op catalogue. Add UI ops (panels, view) as rows in the same catalogue instead of a generated copy, because Gradiance has one process. | Small. |
| 6 | **MCP server as a thin bridge to the control channel** | `craft-mcp` (275 lines), SolidCraft `docs/mcp.md` | `app-cli mcp` speaks MCP on stdio, attaches to a running app (token file, loopback port) or runs headless. Tools are `execute`/`batch` over the command registry. | Gradiance has BRP over HTTP but no MCP server. A small `gradiance-mcp` binary (outside the app) maps MCP tools to `gradiance.op`/`gradiance.query`/`gradiance.ui.*`. It needs no `gradiance-*` dependency at all: it talks JSON to BRP. | Medium. Token auth and the file-access confinement in SolidCraft `docs/mcp.md` are worth copying as rules. |
| 7 | **Screen snapshot tests on lavapipe** | `craft-ui/src/testing/mod.rs` (`Screens`, `egui_kittest` with `snapshot`+`wgpu`); SolidCraft CI (`SOLIDCRAFT_GPU: software`, `SOLIDCRAFT_REQUIRE_GPU: "1"`, apt `mesa-vulkan-drivers libvulkan1`) | Renders a UI closure to PNG with egui_kittest's wgpu renderer on lavapipe and compares to a stored snapshot. CI **fails** (does not skip) when no adapter exists. A global lock serialises wgpu tests to avoid a Vulkan loader SIGSEGV. | Gradiance's `egui_kittest` tests are accessibility-only. Turning on `snapshot`+`wgpu` gives image regression tests for panels without Xvfb. Copy the "require GPU, fail not skip" rule and the GPU test lock. | Small code; CI change; stored PNGs conflict with "screenshots never go in git" (Q3). |
| 8 | **`craft-keymap`** | `craft-libs/crates/keymap` (17 k lines) | Chords, canonical text form, per-platform display, conflicts, user overrides, profiles, import of other apps' key sets, a shortcut editor model. | Replaces the static `interaction::input::default_bindings` table when user rebinding is wanted. The "import other apps' key sets" part gives a **Blender keymap profile**. | Large. Not needed for the Blender look; do it later. |
| 9 | **Docking with a command API** | `craft-ui/src/docking` (model, placement, resize, drop targets, floating, native windows); SolidCraft `docs/panel-foundation.md`, `panel.*` and `layout.*` commands | Every layout change is a typed command with `expected_revision`, refusals instead of guesses, and a saved layout catalogue per workspace. | The command list (`panel.open/close/move/resize/focus`, `layout.save/apply/reset`, `layout.workspace.select`) is a good **spec** for Gradiance's `panel-*` verbs. Do not replace `egui_tiles`: the Craft docking is large and on egui 0.37. | Spec only. |
| 10 | **Action matrix** | SolidCraft `docs/action-matrix.md` | Tries every action on every element of a corpus of real designs, through the same commands the desktop sends; a JSON baseline is a ratchet. | Phase 6 (testing) fits: every registered op × every body in the `examples/` scenes. A passing op that fails later fails CI. | Medium; Phase 6 brief. |
| 11 | **Deterministic mutation testkit** | `solidcraft/crates/testkit/src/lib.rs` | Seeded truncations and byte flips of valid files; a reader must give an error, never a panic. | Scene RON loader in `gradiance-scene`. | Small; Phase 6. |

### 4.2 Pieces I do not recommend

- SolidCraft `ui-kit`, `look`, `theme`: **not permitted** (brand licence, § 1.1).
- `ui-egui` bulk (119 k–397 k lines). Its components assume each app's own
  theme and command types. Take single widgets from `craft-libs/crates/ui`
  only when a brief needs one.
- The `storytold/egui` fork. Gradiance pins crates.io egui through bevy_egui.
- Native-window popout, IME composition tests and RTL layouts: no current need.

---

## 5. Graphite: ideas, no code

Graphite's frontend is Svelte. The Rust side sends widget **descriptions** to it.

- **Widgets as data.** `editor/src/messages/layout/utility_types/widgets/*.rs`:
  each widget is a serialisable struct (`CheckboxInput { checked, icon,
  disabled, tooltip_label, tooltip_shortcut, on_update, on_commit }`). A panel
  is a `Layout` value. For Gradiance this is the path to "keep the UI
  replaceable at the crate boundary" (Future intent 1): a property panel
  described as data could be drawn by egui today and by another shell later.
  It is also how a script could build a panel. **Too large for now**; record it
  as the direction for the inspector.
- **`on_update` vs `on_commit`.** The same split as `precise_drag`'s
  `Commit::Done(old, new)`. This confirms the current seam rule; nothing to change.
- **Input mapper.** `input_mapper/input_mappings.rs` is one declarative table
  from key + modifiers + context to an action message. Gradiance's
  `default_bindings` already has this shape.

---

## 6. MCP and UI testing on lavapipe: the proposed stack

Three tiers. Each tier runs in an agent container on lavapipe.

| Tier | What | Needs Xvfb | Needs the full app | Catches |
|---|---|---|---|---|
| T1 | `egui_kittest` accessibility tests (now) + **image snapshots** (`snapshot`, `wgpu` features) of leaf renderers | no | no | Layout reflow, missing labels, theme regressions |
| T2 | In-app control: `gradiance.ui.inspect`, `ui.click/drag/key/text` by **registry name**, `ui.screenshot` | yes (a window must present frames; I did not test a windowless Bevy run) | yes | Gesture flows, keyboard shortcuts, tool previews, real rendering |
| T3 | `gradiance-mcp` stdio server → BRP | as T2 | yes | Agent-driven sessions; also lets the owner drive the app from an MCP client |

Effect on the current harness (`tools/screenshot/session.sh`): it keeps Xvfb
for the window and drops `xdotool` and `xwd`. The "keyboard does not arrive"
and "egui drops long clicks" hazards go away, because input enters inside the
app and click timing is the app's own.

Lavapipe cost: SolidCraft's CI notes that lavapipe compiles shaders on every
core and needed 16 vCPU for its GPU tests. Gradiance's T1 snapshots draw only
egui (small shaders), so I expect a low cost. **Not measured.**

---

## 7. Constraints these proposals must hold

- **Invariant 4 (egui only in `gradiance-ui`).** Every widget, the registry and
  the egui side of input injection live in `gradiance-ui`. Pure parts (the
  Ctrl-snap step rule, float precision, the menu-search scorer, the
  expression parser) go to lower crates with unit tests (`ui-design.md` § 6).
- **Layer rule.** BRP methods live in `gradiance-script` (`remote.rs`), which
  sits **below** `gradiance-ui`. `gradiance.ui.*` methods need UI state. Two
  shapes keep the DAG: (a) `gradiance-ui` publishes a mirror resource (as
  `PanelStates` does) and queues requests that the UI drains, or (b)
  `gradiance-ui` registers its own BRP method handlers. Option (b) depends on
  whether Bevy 0.19's `bevy_remote` accepts methods after the plugin is
  built. **I did not check this.** The brief must check it first.
- **Invariants 1–2.** Injected input is input: it goes through the same tools,
  so one gesture still commits one command. `ui.menu.invoke` emits the
  intent the menu emits. No new mutation path.
- **Exact pins and `--locked`.** A version bump (bevy_egui 0.42 / egui 0.36) is
  its own reviewed brief, with `tests/boundaries.rs` and `Cargo.lock` in the
  diff. No git dependencies.
- **Attribution.** Vendored Craft code keeps its `MIT OR Apache-2.0` header and
  gets a line in a `NOTICE`/attribution file. The Craft repos have
  `ATTRIBUTION.md` and `NOTICE`; the brief copies the needed lines.
- **Other agents.** `docs/state.md` says one worker at a time on a shared disk.
  Brief 5-1 (units) is in flight, then 4b-0..4b-4. Brief 4b-2 (groups panel)
  and 4b-0 (review-mode panel) edit `gradiance-ui`. The briefs below avoid
  `inspector.rs`, `outliner.rs` and `dock.rs` until 4b-2 lands, and the
  numeric-field briefs wait for 5-3 (unit-aware input), which edits the same
  widget.

---

## 8. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| SolidCraft brand licence (`ui-kit`, `look`, `theme`) | Copying needs written permission | Take SolidCraft code only from MIT/Apache crates. Take widget code from `craft-libs/crates/ui`. |
| GPL contamination from BUI | Licence of the whole app changes | No BUI code or data files. Behaviour only. A reviewer checks each UI PR for copied text. |
| egui 0.35 → 0.37 downport | Copied code does not compile; API drift (VectorCraft's own 0.37 migration plan is long) | Vendor small, self-contained modules only (registry, screenshot rules, snapshot harness). Bump to egui 0.36 first. |
| Future intent 1 (external UI project) | Work on egui chrome may be thrown away | If the Craft family **is** that external project (inference: it matches "UI test and MCP infrastructure"), then the test/control tiers are the overlap and keep their value. Owner question Q1. |
| `orchestration.md`: "Don't polish egui widgets, layouts, or theming beyond usable" | The Blender look conflicts with the current plan | Needs an owner ruling (Q1). The test/control tiers do not conflict. |
| Snapshot PNGs in git | Conflicts with "screenshots never go in git" | Q3. Alternative: store a hash of the image, or store snapshots in CI artifacts only. |

---

## 9. Proposed briefs (for the orchestrator; not written as brief files)

Sized to one worker, one PR each. Phase letter "U" is a placeholder.

Now (no owner ruling needed beyond Q1, low conflict with in-flight work):

1. **U-1 Widget registry.** Port `ui-registry` to `gradiance-ui`; tools and
   the palette publish named handles; `egui_kittest` tests query them.
   About 1 day.
2. **U-2 In-app screenshot + `gradiance.ui.inspect`.** Bevy screenshot with
   the Craft settle rule; the layer-rule check from § 7 first. About 1–2 days.
3. **U-3 Input injection** (`gradiance.ui.click/drag/key/text/wheel`, by
   name or point). Rewrite `tools/screenshot/README.md` to use it.
   About 2 days.
4. **U-4 Snapshot tests on lavapipe** (`egui_kittest` `snapshot`+`wgpu`,
   GPU test lock, CI installs `mesa-vulkan-drivers`). Depends on Q3.
   About 1 day.
5. **U-5 `gradiance-mcp`** stdio server over BRP, token + loopback rules from
   SolidCraft `docs/mcp.md`. About 2 days.

Later (needs Q1, Q2, and after 4b-2 and 5-3 land):

1. **U-6 Blender theme + Inter font** (with the glyph-coverage tests).
2. **U-7 Number-field behaviours** (§ 3.3 rows 1–4, 10), after 5-3.
3. **U-8 Menu search (F3) + status-bar key hints.**
4. **U-9 Areas with an editor-type switch + workspaces** (needs Q4).
5. **U-10 Pie menus, Adjust Last Operation, multi-field drag** (each a
   design note first).

Separate, Phase 6: action matrix over the op catalogue; mutation testkit for
the scene loader.

---

## 10. Questions for the owner (move to review mode when 4b-0 lands)

- **Q1.** Does the Blender-like UI goal override `orchestration.md`'s "don't
  polish egui chrome" rule? And is the Craft family the "external project"
  of Future intent 1?
- **Q2.** Blender's own palette (measured from its documentation), or a
  Gradiance palette in Blender's style? Copying BUI's theme file is not an option.
- **Q3.** May UI snapshot PNGs go in git as test fixtures?
- **Q4.** May dock layout and workspaces persist (deferred in
  `orchestration.md`)? As workstation config, not scene content?
- **Q5.** Blender keys (`N` sidebar, `G`/`R`/`S` grab/rotate/scale, `F3`
  search, `Q` favourites) conflict with the tool keys in `ui-design.md` § 5
  (`N` Tracer, `G` Ground, `R` Prismatic, `S` Select). Keep the Algodoo keys,
  switch to Blender keys, or offer both as keymap profiles (needs `craft-keymap`)?

Review-mode scenes for Q2 and Q5 are natural A/B questions: two themes side by
side, or the same scene with each keymap's status bar.

---

## 11. What I did not check

- Bevy 0.19's screenshot API and whether `bevy_remote` accepts methods added
  after the plugin is built.
- egui 0.36 releases of `egui_tiles`, `egui-snarl`, `egui_plot`, `egui_kittest`.
- Whether a Bevy app on lavapipe presents frames without Xvfb.
- The licence of Blender's theme XML as data (not a legal opinion).
- Any compile of downported code. No code was written.
- The LightCraft on gitea.com (proxy refused it).
