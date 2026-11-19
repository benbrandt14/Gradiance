# F3 search, status bar and pie menus over the registry
Phase U · `docs/unified-surface-plan.md` § 3.4 (menus row), § 7.2 (rows
"F3 search", "Pie menus", "Status bar"), § 10 row 11 · status: in review

Written by the orchestrator, 2026-11-19 (owner ruling "scoped work": a
session may write the brief when the work is in the plan).

## Goal

Three new surfaces, each a **pure projection of the operation registry**
(`gradiance-op`), so a new operation reaches them with no edit here:

1. **F3 search.** F3 opens a search box over every operation a person can
   invoke from the UI. Typing ranks rows by name, menu path, Lisp name and
   doc; Enter (or a click) invokes the operation through `InvokeOp`, the
   same path the Edit menu uses. Disabled rows (enable predicate false)
   show greyed, with the reason when there is one.
2. **Status bar.** A strip at the window bottom shows the active tool's
   mouse and key hints, read from the registry's chord metadata and the
   tool's own hint table; plus the sim state (playing or paused) and the
   selection count.
3. **Pie menus.** A radial menu of up to 8 operation ids, opened by a key
   held over the canvas and released over a slice (or click). One pie for
   tools at least; the rows are operation ids in one table, as
   `KEY_OPS` is.

## Read first

- `docs/ui-design.md` (the design system: glyphs only from
  `ui::fonts::glyph`, theme tokens, `PanelToggle`), and the U-7 and U-8
  Outcomes.
- `crates/gradiance-op/src/registry.rs` (`OpMeta`: `menu`, `key`, label,
  doc, category/seam), `interaction::input::KEY_OPS`, the Edit menu and
  context menu code that invokes ops by id (brief U-2a).
- `docs/unified-surface-plan.md` § 7.2 and § 8 (tests).

## Facts (orchestrator, 2026-11-19, on the 5-5 branch)

- F3 and Q have no binding today (`interaction::input`, the leafwing
  `InputMap`). Bound: Ctrl+Z/Y/A/S/O/G, Delete, Backspace, Space, Escape,
  F, F12, and the tool letters S D B C P H W R T L G K N.
- Owner ruling "keys are out of scope" (2026-11-17): keep the current
  bindings, no keymap work. Adding a binding on a **free** key for a new
  surface does not change a current one; record each new chord as a
  decision.
- *Unverified:* whether every op has an enable predicate, and whether
  tools have a hint table today. Report what exists.

## Approach

1. Scorer in a lower crate (pure, unit-tested; `gradiance-op` or a small
   module beside the registry): subsequence + word-start ranking over
   name, Lisp name, menu path and doc, stable for equal scores.
2. `ui::search`: F3 opens a modal box with the ranked list; arrows and
   Enter; Esc closes. It invokes by id (`InvokeOp`); it never writes an
   intent itself. Registry ops only (no VM built-ins).
3. `ui::status_bar`: a bottom strip (a `PanelToggle`, on by default),
   tool hints + state. Text only, theme tokens.
4. `ui::pie`: Q held over the canvas opens the tools pie; release over a
   slice invokes it; release in the centre cancels. The slices are one
   table of operation ids.
5. Each surface is reachable from a script or the remote surface where
   the plan says so (`panel.*` for the status bar; a `ui.search` op that
   returns the ranked ids is welcome if cheap, for the tests).
6. Tests: scorer unit tests; an `it` test that runs F3, types, presses
   Enter and sees the op's effect (one undo step); a pie test that
   invokes a tool through a slice; screenshots of all three on lavapipe
   (not in git unless a snapshot test uses them).

## Out of scope

Editor areas and workspaces (U-10), Adjust Last Operation (U-11), any
change to an existing key binding, a keymap editor, search over scene
content (bodies, labels).

## Done when

- F3, the status bar and the tools pie work in the app (screenshot
  evidence in the PR).
- The tests above pass; fmt, clippy, doc, boundaries green locally; CI
  green.
- Outcome filled; `docs/state.md` updated on the U-9 lines.

## Outcome

Branch `claude/project-thread-tw1mvl-u9`, PR #272, stacked on 5-5
(#271).

### Facts checked (the brief's "unverified")

- **No enable predicate exists.** `OpMeta` has id, Lisp name, doc, seam,
  menu, chord, undo label and example. F3 uses a dry run instead: the row
  is lowered against the context its click would meet; an error is the
  reason, an output with no write is "nothing it applies to is selected".
- **No tool hint table existed**, and no tool was an operation. The
  palette's `TOOL_GROUPS` held name and letter only.

### Landed

- **Scorer** `gradiance_op::search` (pure): every term must match a
  field; whole field 100, prefix 80, word prefix 60, substring 40,
  subsequence 10–30 (word starts earn a bonus; off for prose fields,
  `Field::exact`); times the field's weight; stable for ties.
  `OpEntry::selection_params`: the params a click supplies (`{}`, or each
  `Target` field as `"selection"`, each `Vec<Target>` as `["selection"]`),
  else `None`.
- **Tool operations** `interaction::tool_ops`: `tool.select` … `tool.tracer`
  (13; Lisp `select-tool` … `tracer-tool`), editor seam, no undo step,
  chord = the tool's letter, menu `Tools/<name>`; they lower to a
  `ToolRequest` that sets `NextState<ToolState>` (what a palette click
  does). Plus the **hint table** `tool_ops::hints` (each tool's gestures,
  script tools included). The keys still switch tools directly
  (`interaction::input` unchanged).
- **F3 search** `ui::search`: candidates = non-query operations with
  `selection_params` (24 today: 11 edit, context and file rows, 13 tools).
  Ranked by name 4, menu path 3, Lisp 3, id 2, doc 1. F3 toggles; arrows,
  Enter, click invoke through `InvokeOp`; Esc closes; the highlight
  follows the first row that can run until an arrow moves it. Rows are
  rebuilt in `refresh_search` (exclusive, `OpContext::build`) while open.
  Panel `search` (View ▸ Search operations, F3 shown). **`ui.search`**
  (query) returns the ranked ids (ids only: whether a row can run depends
  on the doorway, the Lisp doorway has no selection, so the conformance
  test's "every doorway, same output" holds).
- **Status bar** `ui::status_bar`: panel `status`, on by default; tool +
  key from its operation, hints, F3 / Q, selection count, Playing/Paused;
  22 points, small text, `header` token. Drawn before the docks;
  `PanelRects::bottom_inset` now stacks bottom panels and the right and
  bottom docks stand on the bar (`above_bottom`).
- **Tools pie** `ui::pie`: `TOOLS_PIE` (8 ids, clockwise from the top),
  labels and chords from the registry, `slice_at` (pure). Q over the
  canvas (no text focus, no modifier, not over a panel) opens it; release
  over a slice runs it; leave and return to the centre, release: cancel; a
  tap keeps it open for a click; Esc or Q closes.
- **Tests**: scorer (5) and `selection_params` (1) in `gradiance-op`;
  `tool_ops` (2); `ui::search` (2), `ui::pie` (4), `ui::status_bar` (2),
  `bottom_panels_stack_from_the_edge`; `tests/it/ui_surfaces.rs` (7):
  F3 → type `delete sel` → Enter deletes the selected box as one undo
  step; a greyed row says why and Enter skips it, Esc closes; the
  candidate set and `ui.search`; the pie by hold-release (switches to
  Circle, no undo step), hold-return-cancel and tap-click; the status bar
  as a panel. Panel counts in `scripting.rs` and `remote.rs` 12 → 14.
- Docs: `docs/ui-design.md` § 3 (the three surfaces) and § 5 (F3, Q);
  `docs/scripting.md` (tool verbs, `ui-search`, panel names, widget
  names); `docs/ops.md` regenerated (91 → 105 operations).

### Decisions (`docs/review/archive.ron`, `decision: true`)

`search-pie-keys` (F3 and Q, both free; no binding changed),
`tools-pie-slices` (N Select, NE Box, E Circle, SE Polygon, S Cut,
SW Spring, W Hinge, NW Drag), `pie-gestures` (tap keeps it open; the rule
is time-free), `search-candidates` (click-runnable ops, dry-run enable,
weights), `tool-operations` (one operation per tool),
`status-bar-default` (on; content as above).

**Review question** `status-bar-and-pie` (`docs/review/questions.ron`,
scenes `docs/review/scenes/status-bar-and-pie/`, `expected_bodies` [4, 4]):
status bar on (as built) or off; the pie and F3 are tried in both, and
the note invites the pie's size and slices. `toon-highlights` stays open.

### Measured

- Live run on lavapipe under Xvfb (screenshots outside git, in the
  session's scratchpad `shots-u9/`): the status bar shows `Box (B) · Drag
  draw a box · Esc cancel · F3 search · Q tools pie … nothing selected ·
  Paused` and follows the tool; F3 with nothing selected greys the
  selection rows with their reason; with a box selected, `del` ranks
  Delete selection first, Enter deleted it (4 → 3 bodies) and `edit.undo`
  brought it back; the pie opened on a tap of Q, highlighted Box under
  the pointer, and a click on Circle switched the tool (status bar:
  `Circle (C)`); the right dock and the bottom dock end at the bar.
- Typed text reaches the F3 field through `ui.text` (5-3 had seen it miss
  other fields).

### Not checked

- `cargo test --workspace` (CI).
- A real held Q with a real mouse: `ui.key` presses and releases at once,
  so the live run used the tap-and-click path; hold-release is covered by
  the `egui_kittest` tests only.
- Once, a pie click right after a `ui.drag` that started in the centre
  did not pick (the next click did); not reproduced, possibly the two
  injected gestures overlapping.
- Real GPU rendering.

### Checked locally

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
--lib`; `cargo test --test boundaries`; `cargo test -p gradiance-op`,
`-p gradiance-interaction`, `-p gradiance-ui --lib`, `-p gradiance-scene
--lib review`; `cargo test --test it` filtered to `ui_surfaces`,
`op_conformance`, `registry_validation`, `ui_input`, `ui_panels`,
`ui_registry`, `scripting`, `remote`, `client`, `review`,
`script_examples`, `ui_snapshots` (with `GRADIANCE_REQUIRE_GPU=1`; no
snapshot changed). All green. rustc 1.100.0 (the pin).

### For later

- F3 does not list operations with numeric params (fillet merge, spawns,
  `panel.toggle` with its name): a params form, or one row per named
  value (panels, looks), would add them.
- The keys and the palette still pick tools directly, not through
  `tool.*`; routing them is keymap work (out of scope).
- A second pie (view, snapping) needs another free key.
