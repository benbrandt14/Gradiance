# Second screenshot audit: tools, motors, selection, slicing, joints
Audit · owner request 2026-11-18 (second pass) · status: in review

Written by the orchestrator, 2026-11-19. A-1 (#260) and A-2 (#261) did
the first pass on 2026-11-18. Since then, 12 PRs changed the app: the
toon ramp and face field (4b-5, 4b-6), number fields (U-8, U-8b), units
in scripts (5-4), F3 search, status bar and tools pie (U-9), workspaces
and editor areas (U-10), and Adjust Last Operation (U-11). This pass
re-checks the owner's five areas on the new main and checks the new
surfaces.

The owner's request, verbatim:

> take pictures of all operations and menus and identify errors and resolve
> them. Particularly with tool interactions, motors, selection, slicing, and
> multi bodies with joints.

## Goal

Every tool, menu, panel and new surface is exercised once in the real app
(lavapipe + Xvfb, over `--remote` with the `ui.*` operations) and
photographed. Each defect is a row in a Findings table with an evidence
image; small ones are fixed in this PR with a test that failed first.

## Read first

- `docs/briefs/A-1-visual-audit.md` and `A-2-audit-follow-ups.md`
  (method, findings, what was fixed, what was deferred).
- `tools/screenshot/README.md`, `docs/scripting.md` § CLI and MCP,
  `docs/ops.md`; Outcomes of U-9, U-10, U-11, 4b-6.

## Method

1. One build, `tools/screenshot/session.sh start`, driven with
   `session.sh op` / `ui.click`, `ui.click_button`, `ui.drag`, `ui.key` on
   names from `ui.inspect`. Images to `/mnt/project-files/a-3/` (not git),
   with a README listing each in one line.
2. Priority order:
   1. **Tool interactions:** every tool's draw/commit, previews, snapping,
      the status-bar hints for each tool, the tools pie, Adjust Last
      Operation after each tool and op that supports it.
   2. **Motors:** hinge motor on body-to-body and world-pin joints, speed
      and torque display units, sim speed, short-arm speed loss (retained
      item: measure only).
   3. **Selection:** click, Shift-click, box select, groups, select-all,
      the outliner, F3 rows greyed by selection, Ctrl+Z while the pointer
      is over a number field (U-8b found it does not undo).
   4. **Slicing:** the cut tool on boxes, circles, polygons, CSG bodies,
      grouped and jointed bodies; what happens to joints on a cut body.
   5. **Multi-body joints:** chains of hinges, sliders and springs, welds,
      joint inspector, deleting a body in a chain, undo across them.
   6. Every menu and panel once, the three workspaces, the editor-type
      menu, the ramp editor, the review mode questions.
3. For each finding: before image, a failing test where it is testable
   headless, the fix through the existing seams (`CLAUDE.md` invariants),
   an after image.
4. A fix that needs a design change gets a one-line proposal and a
   "needs a brief" row. Interactive judgement calls go to
   `docs/review/questions.ron`.

## Out of scope

New features, redesigns, key binding changes (owner ruling), retained
items that are not bugs (measure only).

## Done when

- Every tool, menu, panel and new surface has at least one image; the
  Findings table is complete; each small fix has a test.
- Gates green locally; CI green. Outcome filled; `docs/state.md` updated
  on the audit lines.

## Findings

Images are in `/mnt/project-files/a-3/` (not in git; one line each in its
`README.md`). 00–65 were taken on the binary built at b93ad2f, 70–81 after
the fixes.

| # | Area | What | Evidence | Status |
|---|---|---|---|---|
| 1 | Slicing | One stroke through **both** bodies of a body-to-body joint: each body's rewiring was staged against the pre-cut world and written whole, so the second write put the first body's despawned id back. The joint pointed at a dead body; the pieces fell apart. | 12, 13, 14 → 70, 71 | **fixed** (test `csg::a_cut_through_both_jointed_bodies_rewires_both_ends`) |
| 2 | Selection | Ctrl+Z (any Ctrl chord) with the pointer resting on a number field did nothing: the field claimed the whole keyboard (U-8b's note). | 25 → 73 | **fixed** (test `ui_input::ctrl_z_over_a_hovered_number_field_undoes`) |
| 3 | Tools / selection | A click a tool used also selected: a polygon closed on the press selected the new polygon on the release; a weld, make-static, rod or tracer click selected the body under it (the click-through looked for three intent kinds on the release frame only). | 04, 08, 34, 47 → 72 | **fixed** (test `ui_input::a_click_that_a_tool_commits_does_not_select`) |
| 4 | Tools | With the Tracer tool active, a click on the UI (a menu, the palette, a panel) placed a free tracer at the pointer: the tool placed on every release, wherever it landed. Undo then showed extra "Spawn Node" steps and New scene could leave a tracer behind. | 53, 58, 60-Scripting → 72 | **fixed** (test `ui_input::a_tracer_is_placed_only_by_a_click_on_the_canvas`) |
| 5 | Panels / workspaces | The Signals workspace opens a bottom dock half the window tall; `PanelRects::bottom_inset` only counted a bottom panel whose top was below the window centre, so the scene camera rendered under the dock and the palette and Adjust panel overlapped it. | 60-Signals → 80 | **fixed** (unit test `a_tall_bottom_dock_still_insets_the_bottom`) |
| 6 | Selection | Context menu: Ungroup was enabled on any selection, a click that did nothing on ungrouped bodies. | 65 → 81 | **fixed** (live only: egui menu) |
| 7 | Motors | World-pin and body-to-body hinge motors (20 rpm, auto torque 409 N·m shown): both turn counter-clockwise; the angle sensor reads the turn. | 15–19 | measured, no finding |
| 8 | Motors | Short arms lose motor speed (retained). No physics change since A-2's sweep; its table stands. | — | measured only (A-2) |
| 9 | Joints | 4-link hinge chain, slider, spring chain, rod, weld; deleting a middle link removes it and its two hinges, one undo restores all three; inspectors for hinge, slider, spring. | 34–41, 43, 44 | no finding |
| 10 | Slicing | Cut tool through a circle, a CSG subtract, a CSG union and a group of two: pieces keep the group; four undos restore the CSG trees. A world pin snapped to an edge midpoint survives a cut of its body. | 11, 30–32 | no finding |
| 11 | Selection | Box select (contained bodies only), Ctrl+G then a click selects the group, Ctrl+A, the outliner rows, F3 greying with and without a selection. | 20–24, 26, 27 | no finding |
| 12 | Selection | One polygon read **Kinematic** in Properties after a Select click (image 78); not reproduced with a fresh box or polygon over the same steps. | 73, 78 | not reproduced; watch |
| 13 | Panels | The joint window covers the right dock, and collapsed it sits over the Outliner tab's editor menu. | 38–41 | retained (A-1 #10) |
| 14 | Tools | Tracer preview and trail dots are circles metres wide. | 47, 55, 57 | retained (5-1) |
| 15 | Panels | Opening a dock or switching workspace re-centres the scene on the smaller pane. | 24, 80 | retained (A-2 #4, needs a brief) |
| 16 | Panels | The Settings window opens with its title bar a few points over the menu bar. | 62, 63 | low; **needs a brief** with the other floating windows: give each a default position below `PanelRects::top_inset` |
| 17 | F3 search | With nothing runnable, the highlight sits on a greyed row (Enter skips it). | 26 | low; as designed in U-9 |
| 18 | Tools | Right-click on an unselected body selects it; the context menu opened only on a second right-click in the injected runs. | 65, 81 | not checked by hand (may be the injection) |

Every tool was used once (Box, Circle, Polygon, Ground, Select, Drag,
Hinge, Prismatic, Spring, Rod, Weld, Cut, Tracer) with its status-bar
hints; the tools pie (55); the File, Edit, View, Help menus (49, 56); the
three workspaces and the editor-type menu (24, 60, 61); Settings and the
ramp editor (62, 63); the Review panel (64); Adjust Last after an
operation (13, 20); Undo Simulate (50).

## Outcome

**In review (2026-11-19, worker).** PR #277 (now on main, after #276).

### Landed

1. **Cuts through jointed pairs** (`command::cut_cmd`): staging splits the
   bodies first, then stages each joint on a cut body once
   (`rewire_joints` over every outcome, `rewire_end` per end), so a joint
   whose two bodies are both cut has both ends on live pieces; the joint
   changes apply after every body is replaced. Undo and redo exact.
2. **Ctrl chords over a hovered field** (`interaction::FieldKeys`, written
   by `ui::capture_pointer_over_ui`): the field keeps its keys (Backspace,
   Delete, minus, Esc, Ctrl+C/V); the editor's other Ctrl chords pass. A
   field with text focus still takes every key. `docs/ui-design.md` § 2.
3. **Click-through after a commit** (`tools::click_select::ToolCommitted`,
   set in `ToolCommitWriters::emit`): a commit anywhere from press to
   release consumes the click; replaces the three-intent check.
4. **Tracer on UI clicks** (`tools::node_tools`): the tool is armed by a
   canvas press and places on that press's release only.
5. **Tall bottom dock** (`ui::PanelRects::bottom_inset`): a bottom panel is
   one that starts at the viewport's left edge (the right dock does not),
   not one whose top is in the lower half.
6. **Context menu Ungroup** greyed unless a selected body is in a group.

### Decisions

`field-ctrl-chords` and `click-through-commits` in
`docs/review/archive.ron` (`decision: true`). In scope, in this Outcome:
the bottom-panel rule (left edge), and the cut stages joints once.

### Measured

- Hinge motors (world pin and body to body) both turn counter-clockwise at
  the default 20 rpm with the derived torque shown; sim speed and the
  short-arm loss were not re-measured (no physics change since A-2).
- Each fix: the new test failed on the code before it (the three `it`
  tests together, the tracer test and the unit test each on its own), then
  passed.

### Not checked

- `cargo test --workspace` (CI).
- Shift-click, right-drag rotate and Ctrl-drag (the injection holds no
  modifier during a pointer gesture); covered headless by `tools::` and
  `array_drag::`.
- Finding 12 (one Kinematic polygon) and finding 18 (first right-click).
- A real GPU; all images are lavapipe.

### Checked locally

rustc 1.100.0 (= `RUST_PIN`). `cargo fmt --all -- --check`; `cargo clippy
--workspace --all-targets -- -D warnings`; `RUSTDOCFLAGS="-D warnings"
cargo doc --workspace --no-deps --lib`; `cargo test --test boundaries`;
`-p gradiance-ui --lib`, `-p gradiance-interaction --lib`,
`-p gradiance-command --lib`; `--test it` modules csg, ui_input, tools,
interaction, joints, ui_surfaces, ui_workspaces, adjust_last, ui_panels,
ui_snapshots (`GRADIANCE_REQUIRE_GPU=1`), replay, booleans, rod, groups,
ui_groups, history, op_conformance (186 passed), review (6).

### For later

- Finding 16: default positions for floating windows below the menu bar.
- Findings 13–15 stay retained.
