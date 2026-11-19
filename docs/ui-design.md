# UI design — the editor's shared vocabulary

The record that did not exist. Every panel was built feature-first, so the
chrome accumulated four ways to write a section header, two glyphs meaning
"close", and nine numeric-entry idioms. This is the vocabulary that replaced
them, plus the rules that decide where a new surface goes.

Scope: `crates/gradiance-ui` only — the one crate allowed to import `egui`
(invariant 4). Interaction rules for the *scene* (tools, gestures, snapping)
live in `docs/roadmap.md` and the tool docs; this is about chrome (its
colours: § 8, brief U-7), plus § 7, the scene look the chrome sits over
(brief 4b-3).

---

## 1. Typography and glyphs

### Fonts are configured; they were not

egui 0.36 bundles four fonts, and the workspace shipped with **no font
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
- Use an icon for a *prominent, repeated* action — the transport, the tool
  palette — where a user learns the picture's **position** and stops reading it.
  Use a glyph for inline chrome (a close ✕, a ▸ disclosure). Use words for
  anything the user must read to understand.

The `Icon` enum is deliberately narrow, and that is a correction rather than an
oversight: it shipped with eight variants picked by guessing which actions wanted
pictures, and five never found a call site. Their natural homes turned out to be
glyph territory (close, zoom-to-fit — both already glyphs) or *menu text*, which
is read rather than recognised. Adding decoration to menu entries to justify an
enum variant is backwards reasoning, so the variants went instead.

Beware the shape of the test that let them survive: `every_icon_path_exists`
iterates `Icon::ALL`, so an unused variant still compiles and still passes. A
green suite is not evidence a variant is *used* — only that its file is on disk.

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

### A quantity field names its dimension, not its unit (brief 5-3)

A field on a physical quantity passes a `Dimension` (`precise_drag_unit`,
edit seam) or a stored `DisplayUnit` (`labelled_quantity`, config seam), never
a unit string. `ui::unit_fields` then shows the value in the unit the
workstation preference picks (degrees or radians; m, cm or mm; the canonical
SI unit otherwise) with the unit after the number, and parses typed entry
through the one parser (`gradiance_units::expr`): `25 cm`, `90 deg`, `2 kN`,
`2*pi`. A bare number is in the shown unit; an entry of another dimension, or
one that does not parse, leaves the value as it was. Values stay SI. A field
whose unit is its own (a hinge motor in rpm) uses `precise_drag_in`. The
reflect grid recognises a quantity newtype by its type name, so a new
settings field gets its unit with no table row; a plain `f32` kept in degrees
names `DisplayUnit::DEGREES` in the grid's table.

### Every number field behaves like Blender's (brief U-8)

One function, `ui::unit_fields::number_field`, carries the behaviours.
`precise_drag*` (edit seam), `labelled_drag`, `labelled_quantity` and the
reflect grid's number rows (config seam) reach it. Panels that still build
a raw `egui::DragValue` (optimizer, node graph, groups, signals, array,
context menu, a few settings rows, integer fields) have only egui's own
Shift and typed numbers until they move onto these helpers; do not add a
new one.

| Gesture | Effect |
|---|---|
| Drag | Scrub; **Shift** = 1/10 speed; **Ctrl** snaps (`gradiance_units::step`: 10° for angles, a tenth of a range's decade, else whole units or about one step per 10 points); Ctrl+Shift = a tenth of the snap |
| Esc during a drag | The value goes back; nothing commits |
| Click, type | A number or maths with units, applied on Enter / focus loss; Esc keeps the old value; a refused entry outlines red and says why for 4 s |
| Hover + Backspace / `-` / Ctrl+wheel | Default / negate / one snap step per notch |
| Hover + Ctrl+C / Ctrl+V | Copy the value with its unit / paste text through the parser |
| Middle-click, right-click | Default; a menu with Copy, Paste, Reset to default |

On the edit seam each of these is **one** commit: a drag on release, a typed
entry on Enter, a key, a wheel notch or a menu row at once, and a cancelled
drag never. A hovered or dragged field claims the keyboard
(`unit_fields::claim_keys` → `KeyboardCaptured`), so Backspace over a field
does not also delete the selection; keys reach a field only while the
pointer is over it, never as global keys. Reset needs a default: config
rows have none, so their Backspace and Reset do nothing.

### Empty states say what to do

"No series selected — pick one above", not "No data". Every `empty_state` in
the crate names the action that fills it. A panel that can be empty on first
run is a panel whose empty state is its onboarding.

---

## 3. Where a surface goes

### Dock pane vs floating window

The ruling generalises the one recorded at `optimize-decision.md:374-386`:

- **Dock pane** — a *view of the model* you keep open while working: Outliner,
  Groups, Properties, Depth, Plot, Node Graph, Script. It earns permanent screen space
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

### Panels are data — and a registry

Every panel resource implements `ui::panels::PanelToggle` (`is_open`,
`set_open`, `toggle`), and `menu::Panels::named` gives each one a stable
lower-case name. That table is the registry: the View menu is a loop over it,
and so are the `panel-show` / `panel-hide` / `panel-toggle` script verbs and the
`panel-open?` read. **Adding a panel is one row** and it appears in the menu and
the scripting API together — a unit test asserts every name has a menu label, so
the two cannot drift.

This is `docs/ui-shell-decision.md`'s "a menu item is a registered op", and it
is deliberately literal: a verb queues a request, and the UI applies it through
`set_open` — the same call a menu click makes. Scripting adds no mutation path.
The read direction is a mirror rather than a dependency, because panel state
lives in `gradiance-ui` and the script layer sits below it.

Names are an API and stay stable; menu **labels** are prose and may be
retitled, which is why `menu_label` maps between them instead of reusing one
string for both.

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
| `B` | Box | | `T` | Spring |
| `C` | Circle | | `W` | Weld |
| `P` | Polygon | | `G` | Ground |
| `K` | Cut | | `N` | Tracer |
| | | | `L` | Rod |

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
- **Looks, on lavapipe** — `tests/it/ui_snapshots.rs` (brief U-5) renders a
  few leaf renderers (tool palette, settings grid, curve editor, plot pane,
  glyph sheet) with the editor's fonts on a CPU Vulkan adapter and compares
  each with a PNG in `tests/snapshots/ui/`. A brief that changes how chrome
  looks regenerates them (`UPDATE_SNAPSHOTS=1 cargo test --test it
  ui_snapshots`) and its reviewer reads the PNG diff. Add a snapshot when a
  new leaf renderer carries a look worth pinning; keep them few and small.

### The node canvas has no persistence

Comment blocks and dragged block positions are **view state on `NodeGraph`**,
lost on restart. That is deliberate while the canvas is in flux: persisting them
means choosing whether layout is document content (undoable, travels with the
file) or workstation config (a separate layout file, keyed to a scene somehow),
and either choice is a format to migrate later. Comments survive reconcile, not
restart.

Note that egui and gizmo draw paths are structurally uncoverable headless
(`desmell-log.md:68`), against a 50% CI line floor — which is another reason to
keep the decisions out of the draw call.

---

## 7. The scene look (brief 4b-3)

The scene — bodies, outlines, backdrop, grounds, lights, and every gizmo
drawn over them — has one source of colour: `gradiance_domain::theme`. The
chrome's tokens are § 8's; they read the scene **accent** (`Ink::accent`,
the selection colour) from here.

### Looks

A `Look` names a direction; `RenderSettings::look` holds the active one
(scene content: saved, one undo step). Applying a look (`look.apply`, or
Settings ▸ Rendering ▸ Look) also resets the render, lighting and scenery
settings to its preset; editing those afterwards keeps the look.

| Look | Name | Backdrop | Pieces | Light | Bands |
|---|---|---|---|---|---|
| Clay studio (default) | `studio` | warm light grey | clay tones (OKLCH L 0.585), dark warm outlines | warm key + cool fill, both shadowing | 4, rim 0.2, glint 0.25 |
| Blueprint | `blueprint` | navy | pale chalk tones (L 0.84), navy ink outlines | one cool key | 3, rim 0.35 |
| Flat graphic | `poster` | paper white | bold tones (L 0.615), black outlines | one key | 2, no rim or glint |

**Why studio is the default:** it is the only look whose two lights make
the toon bands read in a plain 2D view (overlapping shadows step through
all four bands), its mid-light backdrop keeps both dark outlines and
saturated ink legible, and it is the nearest to the toon look the owner
chose in review (`shading`). The owner's pick is open as the review question
`look`.

### The palette is semantic

`appearance::PALETTE` is the studio palette. A body stores one of its ten
fills; under another look the render layer shows that slot in the look's
tone (`SceneTheme::display`), as it does the default outline, neutral fill,
ground and rod. So a red body is red in every look, a custom colour is shown
exactly as authored, and the save file never changes with the look. (The
inspector's swatch row still shows the studio tones; see the 4b-3 Outcome.)

### Ink roles

Gizmos draw with a **role**, never a colour: `theme::Role` (`Accent`,
`Snap`, `Guide`, `Preview`, `Alternate`, `Danger`, `Marquee`, `Rotate`,
`Joint`, `JointOutline`, `Muted`, `Grid`), resolved by the active look's
`Ink`. A tool's `ToolPreview` holds `Stroke`s (a role and an alpha) and is
resolved at draw time. Debug overlays use `theme::debug`, which does not
change with the look.

### Rules the tests hold

- **One source.** `tests/boundaries.rs` (`scene_colours_have_one_source`)
  fails on a colour written as numbers (or a `css::` / `Color32::` name)
  outside the two theme modules (`gradiance_domain::theme`, `ui::theme`).
  Add a token, not a literal.
- **Contrast** (`theme::floors`, WCAG ratio): fill and rod vs backdrop ≥ 3,
  outline vs fill ≥ 3, ink vs backdrop ≥ 3, joint face vs outline ≥ 3,
  ground vs backdrop ≥ 1.2 (a surface, not an object).
- **Distinctness:** palette entries are ≥ 0.045 apart in OKLab (about two
  just-noticeable differences).

### Lighting and bands

Light levels are set against the toon white point (1.0): see
`Look::lighting`. The back plane and grounds are banded like bodies, so a
shadow is a flat step. The camera uses a linear tone curve, so the top band
shows a fill exactly as authored. SSAO stays off: at this scale it changes
nothing visible (4b-3 Outcome); outlines and banded, layered shadows carry
the contact cues instead.

---

## 8. Chrome tokens (brief U-7)

The panels draw with **semantic tokens** from `ui::theme`, never with
colours. `ChromeTheme` is one resolved token set; `visuals()` turns it into
`egui::Visuals`, and `theme::tokens(ctx)` hands it to a panel that paints
itself. `ui/src/theme.rs` is the only file in `gradiance-ui` that may write
a colour as numbers (the one-source scanner, § 7).

| Token | Use | Blender dark |
|---|---|---|
| `panel` | docks, side panels | `#303030` |
| `header` | menu bar, dock tab strips | `#1d1d1d` |
| `window` | floating windows, menus, popups | `#282828` |
| `field` | text/number entry, plot and console backgrounds | `#1d1d1d` |
| `widget` / `widget_hovered` | a button's inner, under the pointer | `#545454` / `#656565` |
| `outline` / `border` | widget outlines, separators / window edges | `#3d3d3d` / `#161616` |
| `text` / `text_strong` / `text_weak` | labels / hovered, pressed, selected / hints | `#e6e6e6` / `#ffffff` / `#9e9e9e` |
| `accent` | the scene look's `Ink::accent`, as authored | studio `#cf5604` |
| `selected` | selection highlight behind `text_strong` | the accent, darkened to 4.5:1 |
| `accent_text` | the accent as text (hyperlinks) | the accent, lightened to 4.5:1 |
| `warning` / `error` / `ok` | report text | `#f0a030` / `#ff7070` / `#78c878` |

Plus the content inks panels paint themselves: `series` (plot lines, the
curve), `handle`, `chart_grid`, console `syntax_*`, the node canvas
(`canvas`, `canvas_dot`, `node`, `node_header`, `node_outline`, `pin_in`,
`pin_out`), the label plates (`overlay`, `overlay_text`), the view cube
(`cube_*`) and the dials (`dial`, `muted`).

### One accent

The chrome has no accent of its own: it is the active scene look's, so a
selected tool, tab or list row and a selected body are one hue, and the
chrome follows Settings ▸ Rendering ▸ Look. The swatch rows (inspector,
Groups panel) show palette slots in the look's tone too.

### Presets

`Chrome` (`gradiance_domain::settings`) names three token sets: `blender`
(the default), `blender-blue` (Blender's own `#4772b3` selection; the
canvas keeps the look's accent) and `stock` (egui's own dark visuals, the
look before U-7). Settings ▸ Interface picks one; so does
`(chrome-apply "…")`. The preset is **workstation config**: it describes
the person at the editor, so it is never undone; and, like `ToolDefaults`,
it is not written into the scene file, so opening someone's scene never
restyles your panels (a workstation file is U-10's). The owner's pick is
the review question `chrome`.

### Rules the tests hold

- **Contrast** (`ui::theme` tests, WCAG): every text token — normal, strong,
  weak, accent text, warning, error, ok, console syntax — reads at ≥ 4.5:1
  on every surface it sits on (panel, window, field, header; strong text on
  the hovered widget and on the selection), for both Blender presets over
  every scene look. Text over the scene sits on the `overlay` plate and
  reads at ≥ 4.5:1 over every look's backdrop. Plot series and the curve
  handle are marks: ≥ 3:1 on `field`. `stock` is exempt: it is egui's, kept
  for comparison (its weak text is 2.7:1).
- **One accent:** the selection keeps the accent's hue (channel order) in
  every look.
- **Lean Blender:** the default surfaces are neutral greys, and no two
  adjacent surfaces differ by 2:1 or more (low-contrast chrome).
