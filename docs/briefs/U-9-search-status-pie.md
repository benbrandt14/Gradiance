# F3 search, status bar and pie menus over the registry
Phase U · `docs/unified-surface-plan.md` § 3.4 (menus row), § 7.2 (rows
"F3 search", "Pie menus", "Status bar"), § 10 row 11 · status: ready

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

*(worker fills in)*
