# Groups as editing layers: the panel
Phase 4b · owner direction 2026-11-16 (quoted in 4b-1) · status: in review
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

Worker, 2026-11-18, branch `claude/project-thread-7kscb7`. Everything in
Goal and Approach landed; no split was needed.

**What landed**

- `crates/gradiance-ui/src/groups.rs`: the `GroupsPanel` resource
  (`PanelToggle`; row `"groups"` in `Panels::named`, menu label "Groups",
  a `Pane::Groups` tab in the right dock), the pure `GroupsModel::build`
  (rows in tree order with depth, name, member count and first-member
  swatch; the kinds and palette slots present), and the leaf renderer
  `groups_section(ui, &model, &mut panel, &mut GroupsOutput)`.
  - Rows: click selects the group and makes it active; double-click (or the
    "Rename" button) renames: `group.rename` on Enter or focus loss, Esc
    cancels.
  - Edit: 10 palette swatches (`group.set_color`); property picker over
    `BODY_PROPS` (doc as tooltip) with a `precise_drag` value plus "Apply"
    (`group.set`), or a checkbox for a flag property; From/to, Linear or
    Random+seed, Order by (`group.distribute` / `group.scatter`); Split by
    colour / kind / island and Dissolve (`group.decompose`).
  - Filter: chips for the kinds and palette colours in the scene, grouped,
    ungrouped, and removable property ranges; "Clear"; "Group selection"
    (the Ctrl+G invocation, `key_invocations(EditorAction::Group)`).
  - Every widget is published as `groups:<name>` (`groups:row:<i>`,
    `groups:colour:<swatch>`, `groups:prop`, `groups:prop:<prop>`,
    `groups:distribute`, `groups:split:<key>`, `groups:chip:<label>`,
    `groups:rename-button`, ...), so `ui.click` drives the panel.
- Selection: a row click is the filter `group:<i>`; a chip change writes
  its clause list. Both are a `GroupsFilter` message, which
  `apply_groups_filters` (exclusive, installed headless) runs as
  `body.where` against a fresh `OpContext` and applies through
  `SelectTransition::SetBodies` (empty list: `Clear`).
- `command::body_prop::BodyProp::is_flag` (one method, so a new flag
  property needs only its table row and the match there).
- `inspector::swatch_color32` is `pub(crate)` (shared swatch colour).
- `docs/ui-design.md`: Groups listed as a dock pane. `docs/ops.md` and
  `docs/scripting.md` unchanged (no operation changed).

**Five tasks: clicks, before and after** (before = today's menus, viewport
and console; after = the panel, open)

| Task | Before | After |
|---|---|---|
| 1. Recolour a group | 2 clicks (a member in the viewport, a Properties swatch), and only when a member can be found and Properties is open; else console `(group-set-color 0 r g b)` | 2 clicks: row, swatch |
| 2. Ramp densities across a row of boxes | console only: open it, type `(group-distribute 0 "density" 0.5 5 "x")` | 4 clicks: row, property picker, `density`, Distribute (+ typing lo/hi when not 0.5..5) |
| 3. Select every red body | shift-click each red body (N clicks); `bodies-where` returns indices but cannot select | 1 click: the `red` chip |
| 4. Split a mixed group by colour | console `(decompose 0 "color")`; or ungroup, select each colour, Ctrl+G each (2 + N clicks) | 2 clicks: row, By colour |
| 5. Name a group | console only: `(name-group 0 "wheels")` | double-click row (or row + Rename), type, Enter |

**Screenshots** (ad hoc, not in git; `/mnt/project-files/4b-2/`, driven
by `ui.click` on `groups:` names over `--remote`): `0-overview.png` (two
groups: five boxes, four red/blue circles); `1-recolour.png` (row 0 active,
amber swatch: the boxes are amber, the edit section shows); `2-ramp-density.png`
(picker on density, Distribute: `body.where density:0.4..0.6` and
`4.9..5.1` each return one box, the left- and rightmost);
`4-split-by-colour.png` (Groups (3): the circles split into a red and a
blue group); `3-select-red.png` (the red chip outlined, exactly the two red
circles selected); `5-name-group.png` (row 0 reads "boxes"; `group.list`
returns `["boxes", "", ""]`). `inspect.json` is the `groups:` widget list.

**Decisions (in scope; reversible)**

- **Selection expands to outermost groups**, as every `SetBodies` does: a
  nested group's row selects its outer group, and a chip on a body in a
  mixed group selects the whole group. Task 3 above is after task 4 for
  that reason. Changing it needs a non-expanding transition in
  `gradiance-interaction` (out of scope).
- **Active group is panel state, by id**; ops get its index (id order) at
  click time. It is not derived from the viewport selection.
- **Chips apply at once** (live filter), so task 3 is one click; Clear (or
  untoggling the last chip) clears the selection.
- **A "Rename" button** beside the edit header: an injected pointer at
  lavapipe frame rates cannot double-click, and it is the keyboardless path.
- **Distribute bounds and the range builder use plain `DragValue`s**: they
  are panel parameters for a button, not edits; the set value is the one
  `precise_drag` (commits on release, one `group.set`).

**Tests**

- `crates/gradiance-ui/src/groups.rs` unit: tree order/depth/names/counts,
  kinds and palette; rename commits once on Enter with the group index
  (kittest); flag vs scalar.
- `tests/it/ui_panels.rs::groups_panel_rows_picker_and_chips_emit_their_operations`
  (kittest): row, swatch, picker, Distribute (linear and random), By kind,
  a chip and Group selection record the exact `InvokeOp`s / filter.
- `tests/it/ui_snapshots.rs::snapshot_groups_panel` + committed
  `tests/snapshots/ui/groups_panel.png`.
- `tests/it/ui_groups.rs` (new, intent level; headless has no egui pass so
  `ui.click` cannot reach the panel): the panel drawn by kittest over a
  model built from a headless app, its output written into the app: the
  red chip selects exactly the red bodies and untoggling clears; a row
  click selects the group, then By colour gives two groups in one undo step.
- `tests/it/remote.rs`: the panel count is 12 and the known-name list
  includes `groups`.

**Gates run locally** (rustc 1.100.0): `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --lib`,
`cargo test --test boundaries`, `cargo test -p gradiance-ui --lib`,
`cargo test -p gradiance-command --lib body_prop`,
`GRADIANCE_REQUIRE_GPU=1 cargo test -p gradiance --test it -- ui_ review
groups remote op_conformance registry_validation` (62 passed): all green.
**Not run:** `cargo test --workspace` (CI). Not checked: dock widths below
260 points (rows wrap; not inspected), a double-click by a real mouse
(covered by the egui response, not by a test).
