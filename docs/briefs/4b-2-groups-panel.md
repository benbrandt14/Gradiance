# Groups as editing layers: the panel
Phase 4b · owner direction 2026-11-16 (quoted in 4b-1) · status: ready
(revised 2026-11-18 by the orchestrator, after U-3b: on the registry)

## Goal

A **Groups** panel makes the 4b-1 op surface fast to use. Think of a layers
panel in a drawing tool:
- every group is a row: name, member count, a colour swatch, nesting shown
  by indentation;
- clicking a row selects the group;
- double-clicking renames it.

From the panel, without hunting through menus, the user can:
- edit a parameter of the whole group: pick the property, set a value, one
  undo step;
- distribute a parameter across it: property, lo/hi, mode, sort key;
- filter the scene into a selection with quick chips (kind, colour, ungrouped,
  property range), then make a group from the result;
- decompose a group (by colour, kind, connectivity) or dissolve it.

The workflow must be **clear and efficient**. Measure it: the Outcome lists
the clicks for these five tasks, before and after:
1. recolour a group;
2. ramp densities across a row of boxes;
3. select every red body;
4. split a mixed group by colour;
5. name a group.

## Read first

- Brief 4b-1's Outcome: the verbs, the shared property table, the group
  index, names.
- `docs/ops.md` rows `group.*`, `body.where`, `body.set_color` (the
  operations every panel action invokes).
- `docs/ui-design.md` (glyphs from `ui::fonts::glyph`, `precise_drag` on the
  edit seam, `panels::PanelToggle` for a new panel).
- `crates/gradiance-ui/src/outliner.rs` (the closest list panel), `menu.rs`
  (`Panels::named`) and `panel_ops.rs` (U-3b: a new `Panels::named` row is
  reachable as `panel-show "groups"` with no extra code).
- `tests/it/ui_panels.rs`, `ui_snapshots.rs`, `ui_input.rs` (the three test
  tiers: kittest leaf renderers, lavapipe snapshots, injected input).

## Facts (orchestrator, 2026-11-18, on main after U-3b)

- **Every panel action is an `InvokeOp`** (`gradiance_op::InvokeOp::with(id,
  json!{…})`), the same message the Edit menu writes (U-2a). The panel
  builds params and nothing else; no intent is written directly, no logic a
  new shell would need lives in the panel (`orchestration.md` Future
  intent 1). Ids: `group.list`/`group.members`/`group.name` (reads — or the
  same data through `domain::group::GroupIndex` + `GroupNames` for drawing),
  `group.rename`, `group.set`, `group.set_color`, `group.distribute`,
  `group.scatter`, `group.decompose`, `group.create`, `group.ungroup`,
  `body.where`.
- **The property picker** lists `command::body_prop::BODY_PROPS` (name, doc
  as tooltip). A new property is one table row, and the panel follows.
- **Selection** lives in `gradiance-interaction`. A filter result (indices
  from `body.where`, run against an `OpContext`) becomes a selection through
  `interaction::selection::SelectTransition::SetBodies`, the sanctioned seam.
  No `select` operation exists; adding one is out of scope.
- **Decision (orchestrator, recorded in `docs/review/archive.ron`):** a new
  `groups` panel, not an outliner mode. The outliner is an object tree; the
  group rows carry edit and distribute controls that would crowd it. The
  outliner keeps its group nodes as they are.
- Name widgets for `ui.inspect` with a `groups:` prefix (U-3 naming
  `<area>:<label>`), so the five tasks can be scripted with `ui.click`.

## Approach

1. `groups` panel: a `PanelToggle` resource and one `Panels::named` row.
   Rows from `GroupIndex` + `GroupNames`; click selects, double-click
   renames (`group.rename`).
2. Edit / distribute row: property picker over `BODY_PROPS`, a value
   widget by property type (`precise_drag` for scalars, a checkbox for
   booleans, a colour button for `group.set_color`); distribute lo/hi, mode
   (linear / random seed), sort key.
3. Filter chips → `body.where` clause string → selection; "group
   selection" (`group.create`); decompose menu (`group.decompose`).
4. Tests: a kittest test of the panel's leaf renderer (rows, picker,
   chips emit the right `InvokeOp`s); one lavapipe snapshot of the panel
   (committed fixture, `UPDATE_SNAPSHOTS=1`); an integration test that
   drives one task through `ui.click` by name in `tests/it/ui_input.rs`
   style if the headless app allows it, else intent-level.
5. Screenshots of the five tasks in the running app via
   `tools/screenshot/`, described in the PR (ad-hoc, not in git).

## Out of scope

- New properties, physical defaults (4b-4), theme (4b-3).
- A `select` operation, a label filter for `bodies-where` (needs a mirror
  from `gradiance-script`), the fast-drag offset bug.
- Outliner changes beyond what the panel needs to coexist.

## Done when

- The five-task click table in the Outcome (before = menus/console today,
  after = the panel).
- The tests in Approach 4; fmt, clippy, doc, boundaries, `-p gradiance-ui`
  and the touched `tests/it` modules green locally; CI green.
- `docs/scripting.md`/`docs/ops.md` unchanged unless an op changed; the
  panel listed in `docs/ui-design.md` if it lists panels. Outcome filled;
  `docs/state.md` updated.

## Outcome

*(worker fills in)*
