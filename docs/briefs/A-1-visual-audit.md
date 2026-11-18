# Screenshot audit of operations and menus, with fixes
Audit · owner request 2026-11-18 · status: in progress

Written by the worker (owner ruling "scoped work", `docs/state.md`: a
session may write the brief when the work is in the plan; a bug found by a
screenshot gets a brief, as 2-5 and 4-4 did). Stacked on 4b-4 (#259) →
U-7 (#258) → 4b-3 (#257).

The owner's request, verbatim:

> take pictures of all operations and menus and identify errors and resolve
> them. Particularly with tool interactions, motors, selection, slicing, and
> multi bodies with joints.

## Goal

Every tool, every menu and every panel is exercised once in the real app
(lavapipe + Xvfb, driven over `--remote` with the `ui.*` operations) and
photographed. Each defect found is a row in the Findings table below, with
an evidence image; the small ones are fixed in this PR with a test that
failed first.

## Read first

- `tools/screenshot/README.md` (the harness), `docs/scripting.md` § CLI and
  MCP (`gradiance op …` against a `--remote` app), `docs/ops.md`.
- Briefs U-3, U-4 (widget names, `ui.inspect`, input injection), 4b-4's
  Outcome (bugs seen, not fixed).

## Method

1. One build, `tools/screenshot/session.sh start` (`--remote`), driven with
   `session.sh op` / `ui.click`, `ui.drag`, `ui.key` on names from
   `ui.inspect`. Images go to `/mnt/project-files/a-1/` (not git), with a
   README listing each in one line.
2. Priority order: (1) tool interactions, (2) motors, (3) selection,
   (4) slicing/cutting, (5) multiple bodies with joints, then (6) every
   menu and panel once.
3. For each finding: a before image, then a failing test where the bug is
   testable headless, the fix through the existing seams (invariants in
   `CLAUDE.md`), an after image.
4. A fix that needs a design change is recorded as "needs a brief" with a
   one-line proposal. Interactive judgement calls go to
   `docs/review/questions.ron`.

Known items to verify first: world-pin motor sign reversed vs a
body-to-body hinge (retained "world-pin angle sign (2-1)"); `motor_gain`
ignores `SimSettings::speed`; a fast select-tool drag lands a body offset
by half its size; the colour chip on a mixed group selects the whole
group; the `look` review scenes' `colour.scm` paints wrong bodies when
indices are read before spawns land.

## Out of scope

New features, redesigns of a tool or panel, keys (owner ruling), the
retained items that are not bugs.

## Done when

- Every tool, menu and panel has at least one image; the Findings table is
  complete.
- Each fixed finding has a test (where headless-testable) and an after
  image.
- fmt, clippy, doc, boundaries and the touched `-p` / `it` modules green;
  the rest in CI.

## Outcome

_(filled at handoff)_
