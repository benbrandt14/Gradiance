# Recipe audit — files touched vs. the recipe's ideal

The accretion recipes (`docs/agent-context.md` §6) each promise a bounded
footprint. This audit measures the **most recent landed feature of each kind**
against that promise, from `git show --stat`. Where the real count exceeds the
ideal, the delta is classified as *inherent* (UI-is-part-of-the-step policy,
test files) or *accidental ceremony* (extra registration sites, hop-through
plumbing) — the accidental part is what the Phase-3 de-smell pass removed or
logged (`docs/desmell-log.md`).

Counting rules: `src/` files only; `docs/roadmap.md` bookkeeping and test
files are listed but not counted against the ideal (tests are wanted, not
ceremony).

## 1. A new edit / command

*Ideal:* 4 files — `command/intent.rs` (+ name constant), `command/mod.rs`
(register), the new `command/*_cmd.rs`, `command/dispatch.rs`. Plus an emitter
(UI or tool) and a test.

*Most recent:* `MergeCommand` (`b648ba2`, M13 batch). Isolating the merge
slice: `intent.rs`, `mod.rs`, `merge_cmd.rs`, `dispatch.rs`, plus the
context-menu emitter (`ui/context_menu.rs`) and `tests/csg.rs`.

**Actual: 5 src files vs. ideal 4 (+1 emitter, inherent).** The recipe holds.
No accidental ceremony: the four command-layer touches are the four the recipe
names. Verdict: **on budget**.

## 2. A new tool

*Ideal:* 2 files — the new `interaction/tools/*_tool.rs` implementing
`DraftTool`/`ManipTool` and its registration in `interaction/tools/mod.rs`.
Plus a hotkey and a toolbar button.

*Most recent (draft tool):* the strut tool half of `9553ab7`. Files:
`tools/strut_tool.rs`, `tools/mod.rs` — **and** `core/states.rs` (a
`ToolState` variant), `interaction/input.rs` (an `EditorAction` variant + its
`tool()` match arm + the `TOOLS` array + a binding), `ui/toolbar.rs` (button).

**Actual: 5 src files vs. ideal 2 (+hotkey +button).** Two of the extra three
are inherent (a tool must appear in the state enum and the toolbar). The
accidental part was `input.rs` needing **three** touches for one tool (enum
variant, `tool()` match arm, `TOOLS` array) — a maintenance triple that can
drift independently. **Removed in Phase 3:** `EditorAction::Tool(ToolState)`
now carries the tool state directly, deleting the match and the array; a new
tool touches `input.rs` in exactly one place (the key binding).

## 3. A new joint / constraint

*Ideal:* 5 files — `domain/joint.rs` (the `JointKind` variant),
`physics/joint_sync.rs` (derive), `render/joint_viz.rs` (gizmo),
`ui/joint_inspector.rs` (section), a context-menu path. UI is part of the
step by policy, so all five are inherent.

*Most recent:* `JointKind::Spring` (strut, `9553ab7`): 13 files. Subtracting
the stacked *tool* recipe (5, audited above) and tests/roadmap: `domain/joint.rs`,
`physics/joint_sync.rs`, `render/joint_viz.rs`, `ui/joint_inspector.rs`,
`ui/settings.rs` (debug toggle), `render/debug_viz.rs` (1-line overlay hook).

**Actual: 6 src files vs. ideal 5.** The two 1-line debug-overlay touches are
the only overage — cheap, and proportionate. Verdict: **on budget**; the
perceived weight of "adding a joint" is really "adding a joint *and* a tool"
(two recipes, ~11 files), which is the policy cost of UI-per-step, not
indirection.

## 4. A new plottable / queryable quantity

*Ideal:* 1–2 files — the signal added to the `physics::queries` facade as the
feature lands, then one line in the plot panel's named-signal store.

*Most recent:* joint length/angle plotting (`9729bb3`): `src/ui/plot.rs`
only — but the joint signals were computed **in the panel**, not added to
`physics::queries`.

**Actual: 1 file vs. ideal 2 — under budget in the wrong direction.** The
cheapness was seam-skipping: a signal that lives in the plot panel is
invisible to scripts and probes (the "sensor" leg of the P2 dataflow). This is
recipe drift rather than ceremony. **Follow-up recorded** in
`docs/desmell-log.md`: when the next signal lands, move the joint
length/angle computation behind `physics::queries` (it already holds
`speed_of`/`height_of`-style reads).

## 5. A new script verb

*Ideal:* 3 files — `script/registry.rs` (OpSpec under a `name` constant),
`script/bridge.rs` (builtin; for edits also a row in `edit_bindings()`),
a test.

*Most recent:* `spawn-ground` + two query verbs (`33ce055`):
`script/registry.rs`, `script/bridge.rs`, `tests/scripting.rs`, plus
`domain/props.rs` (`BodyPhysics::fixed()` helper the verb needed).

**Actual: 3 src files vs. ideal 3** (the `props.rs` helper is feature logic,
not ceremony). Verdict: **on budget** — and now guarded: the
registry-validation test (`tests/it/registry_validation.rs`) fails CI when a
catalog entry, builtin, or edit binding is missing, so the lockstep no longer
depends on the author remembering the recipe.

## Summary

| Recipe | Ideal (src) | Actual (src) | Accidental ceremony | Action |
|---|---|---|---|---|
| New command | 4 | 5 | none | — |
| New tool | 2 (+2 registration) | 5 | `input.rs` triple (variant/match/array) | removed (Phase 3) |
| New joint | 5 | 6 | none (1-line debug hooks) | — |
| New plottable | 2 | 1 | seam skipped, not ceremony | follow-up logged |
| New script verb | 3 | 3 | none | guarded by validation test |

The recipes are honest. The one real ceremony find (tool registration triple)
is fixed; the one drift find (plot signals bypassing the read facade) is
logged for the next signal's landing rather than churned now.
