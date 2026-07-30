# UI design — the editor's shared vocabulary

The record that did not exist. Every panel was built feature-first, so the
chrome accumulated four ways to write a section header, two glyphs meaning
"close", and nine numeric-entry idioms. This is the vocabulary that replaced
them, plus the rules that decide where a new surface goes.

Scope: `crates/gradiance-ui` only — the one crate allowed to import `egui`
(invariant 4). Interaction rules for the *scene* (tools, gestures, snapping)
live in `docs/roadmap.md` and the tool docs; this is about chrome.

---

## 1. Typography and glyphs

### Fonts are configured; they were not

egui 0.35 bundles four fonts, and the workspace shipped with **no font
configuration at all**, so it inherited the stock families: Monospace is
Hack → Ubuntu-Light → NotoEmoji → emoji-icon-font, Proportional the same list
**without Hack**. Nine of the codepoints the UI drew resolved in none of them
and rendered as tofu — including `⌂` in the always-visible transport strip.

`ui::fonts::install` appends Hack to the Proportional family, which is what
makes the arrows, media symbols and maths signs below resolve in body text.

### The symbol vocabulary is closed

`ui::fonts::glyph` is the **only** place a non-ASCII codepoint enters the UI,
and two tests keep it that way:

- `every_glyph_renders_in_the_proportional_family` proves each listed glyph has
  an actual outline in the installed fonts (with an `'A'` control assertion, so
  a font that failed to load shows up as a failure rather than as a pass).
- `no_source_file_uses_an_unlisted_glyph` scans the crate's source and fails on
  any non-ASCII character not in the list. It found eight sites on its first
  run that both an audit and a manual sweep had missed.

**Adding a symbol** means adding it to `glyph`, not typing it into a panel. If
the coverage test then fails, the glyph is not available and the answer is a
different glyph or an icon — not shipping a box.

Some obvious choices are absent from egui's fonts and have substitutes:

| Wanted | Absent | Used instead |
|---|---|---|
| close | `✕` U+2715, `✖` U+2716 | `×` U+00D7 (`glyph::CLOSE`) |
| play | `▶` U+25B6 | `⏵` U+23F5 (`glyph::PLAY`) |

A canary test (`stock_egui_really_does_lack_these_glyphs`) records *why* the
`install` step exists, so it can be simplified if egui ever bundles wider
coverage rather than being cargo-culted forever.

### Icons for prominent actions

`assets/icons/` holds an Algodoo-style PNG set. `ui::icons` is the registry:
an `Icon` enum with an asset path, loaded once at startup and registered with
`EguiUserTextures`.

- `icon_button` / `icon_text_button` take a **required** `label`. It is the
  hover text, and it is the fallback when the texture is missing — which is
  what makes the palette testable headless (`tests/it/ui_panels.rs` reads the
  labels).
- Use an icon for a *prominent, repeated* action (transport, tool palette,
  pack, array). Use a glyph for inline chrome (a close ✕, a ▸ disclosure).
  Use words for anything the user must read to understand.

---

## 2. The widget vocabulary

`ui::widgets` is the design system. Prefer these over raw egui:

| Helper | Use for |
|---|---|
| `section_header` | The bold label starting a group. Replaced four idioms. |
| `hint` | Secondary explanatory text (weak, small). |
| `empty_state` | "Nothing here yet", with what to do about it. |
| `close_button` | The one ✕, one size, one meaning. |
| `labelled_drag` / `labelled_drag_u32` | A labelled numeric row on the **config** seam. |
| `precise_drag` / `precise_drag_unit` | A numeric row on the **edit** seam. |
| `node_kind_editor` | Behavior-node fields, shared by three hosts. |

Each returns an `egui::Response` so `.on_hover_text(…)` chains — a helper that
returns `()` silently forbids a tooltip, which is how hints ended up inline as
permanent grey sentences.

### `precise_drag` vs `labelled_drag` — pick by seam, not by looks

This is the distinction that matters and it is invisible in the rendering:

- **Authored state** (a body's width, a joint's stiffness) is edited through
  intents and is undoable. It needs `precise_drag`, which reports
  `Commit::Done(old, new)` **once**, on release or focus loss — so a drag is
  one undo step, not one per frame.
- **Config state** (grid spacing, solver weights, plot options) is written
  directly to a settings resource. There is no undo record to write into, so
  gesture bookkeeping has nothing to do; `labelled_drag` reporting "changed"
  is exactly right.

Converting every site to `precise_drag` would be wrong, not merely wasteful.

### Empty states say what to do

"No series selected — pick one above", not "No data". Every `empty_state` in
the crate names the action that fills it. A panel that can be empty on first
run is a panel whose empty state is its onboarding.

---

## 3. Where a surface goes

### Dock pane vs floating window

The ruling generalises the one recorded at `optimize-decision.md:374-386`:

- **Dock pane** — a *view of the model* you keep open while working: Outliner,
  Properties, Depth, Plot, Node Graph, Script. It earns permanent screen space
  because it changes as the scene does.
- **Floating window** — a *rulebook* you open, dial in, and close: Settings,
  the Optimizer's full config, Array options. It is modal in attention if not
  in input, and it should not cost dock width when you are not using it.

The test: if it would still be worth looking at when you are not thinking about
it, it is a pane.

### Two views of one model belong together

The Signals pane was retired for this reason. It edited the same graph the node
canvas draws, from a different tab, with neither able to see the other. The
list now sits beside the canvas inside the Node Graph pane: the canvas is the
surface you draw on, the list is where names, compile errors, and the edits
with no gesture (rename a binding, retarget a sink, delete a param) live.

### Panels are data

Every panel resource implements `ui::panels::PanelToggle` (`is_open`,
`set_open`, `toggle`), so chrome that offers panels — the View menu, dock tab
close buttons — is a **table** rather than a branch per panel. Adding a panel
is a row.

Before the trait there were two idioms (a `pub open: bool` field, and a private
field behind hand-written accessors) and every consumer paid for both. The
`impl_panel_toggle!` macro writes the implementation for the common case.

### Docks preserve layout

`ui::dock_sync::sync_panes` adds and removes individual tiles. It exists
because both docks used to call `egui_tiles::Tree::new_tabs()` whenever the
open set changed, which rebuilds a **flat tab strip** — so opening the console
discarded whatever split you had arranged. Never rebuild the tree to change
which panes are in it.

Each tab's ✕ turns the section's View-menu toggle off, so the two never
disagree about what is open.

---

## 4. Write seams (the rule chrome is built around)

*Reads are total; writes are seam-mediated.* Any panel may read any component
or resource. Every write goes through exactly one of three seams:

| Seam | How | Undoable | Saved |
|---|---|---|---|
| **Edit** | emit an intent → `command::dispatch` | yes | yes |
| **Config: scene content** | write the settings resource | yes (settled edits) | yes |
| **Config: workstation** | write the settings resource | **no** | yes |
| **EditorState** | write the panel's own resource | no | no |

The scene-content / workstation split is by *what the setting describes*, not
where it is edited: `SimSettings` and the signal graph are part of the
document; `GridSettings` and `SnapConfig` belong to the person, so reverting an
edit must never move someone's grid. The split lives in one place —
`scene::records::EnvironmentRecord::scene_content_eq` / `apply_scene_content`
— and a new settings resource must be classified there.

A panel never calls `get_mut` on an authored component. If a UI change seems to
need that, it needs a command instead.

---

## 5. Keybindings

Authoritative source: `interaction::input::default_bindings`. This table is a
copy for reading; the code wins.

### Tools

| Key | Tool | | Key | Tool |
|---|---|---|---|---|
| `S` | Select | | `H` | Hinge |
| `D` | Drag | | `R` | Prismatic |
| `B` | Box | | `T` | Strut |
| `C` | Circle | | `W` | Weld |
| `P` | Polygon | | `G` | Ground |
| `K` | Cut | | `N` | Tracer |

### Editing and view

| Key | Action |
|---|---|
| `Ctrl+Z` / `Ctrl+Shift+Z`, `Ctrl+Y` | Undo / Redo |
| `Delete`, `Backspace` | Delete selection |
| `Ctrl+A` / `Esc` | Select all / deselect |
| `Space` | Play / pause |
| `F` | Toggle the scale frame (local ↔ global) |
| `Ctrl+G` / `Ctrl+Shift+G` | Group / ungroup |
| `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+O` | Save / Save As / Open |
| `F12` | Snapshot |
| `` ` `` | Script console |
| `\` | Plot |

### Modifiers during a gesture

| Modifier | Effect |
|---|---|
| `Ctrl` + drag a scale handle | Array-repeat instead of scale (handles turn green with a ghost) |
| `Shift` + drag empty node canvas | Box-select blocks |

Every shortcut that toggles a panel is also shown in the View menu, right-
aligned on its row. A binding that exists only in code is a binding nobody
finds — the false `Tracer (Y)` hint sat in the palette for months while `Y` was
actually Redo.

---

## 6. Testing chrome

UI holds no decisions worth testing, so the tests target the two things that do
break:

- **Pure projections** — anything that decides *what* to draw is a free
  function with unit tests: `visible_series`, `depth_lines`, `line_changes`,
  `drag_point`, `sync_panes`, `scene_rect`.
- **Layout, headless** — `egui_kittest` (`tests/it/ui_panels.rs`) renders leaf
  renderers and queries the accessibility tree, which catches reflow and
  missing-label regressions. A panel that only exists as a Bevy system is
  covered by its intent-level tests instead.

Note that egui and gizmo draw paths are structurally uncoverable headless
(`desmell-log.md:68`), against a 50% CI line floor — which is another reason to
keep the decisions out of the draw call.
