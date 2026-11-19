# Second screenshot audit: tools, motors, selection, slicing, joints
Audit · owner request 2026-11-18 (second pass) · status: ready

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

*(worker fills in: # · area · what · evidence · status)*

## Outcome

*(worker fills in)*
