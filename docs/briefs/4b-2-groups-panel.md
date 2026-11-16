# Groups as editing layers: the panel
Phase 4b · owner direction 2026-11-16 (quoted in 4b-1) · status: ready
(after 4b-1)

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
- `docs/ui-design.md` (the design system: glyphs from `ui::fonts::glyph`,
  `precise_drag` on the edit seam, `panels::PanelToggle` for a new panel).
- `crates/gradiance-ui/src/outliner.rs` (the closest existing list panel)
  and `menu.rs` (`Panels::named`).
- `docs/orchestration.md` § Future intent 1. The UI will be replaced: the
  panel holds **no logic** a new shell would need. Every action is a 4b-1
  intent or verb call.

## Facts (orchestrator, 2026-11-16)

- The UI may change freely (owner). The outliner may adopt group rows instead
  of a new panel, if that is clearer. Say which, and why.
- Selection lives in the UI layer. The panel turns a filter's index list into
  a selection through the sanctioned `SelectTransition` seam.

## Approach

1. Groups panel (or outliner group mode), a `PanelToggle`, and a `panel-*`
   verb through `Panels::named`.
2. Edit / distribute row: a property picker from 4b-1's shared table, value
   widgets by property type.
3. Filter chips → selection; "group selection"; decompose menu.
4. Screenshots of each of the five tasks done in the running app via
   `tools/screenshot/`: before/after for each.

## Out of scope

New properties, physical defaults (4b-4), theme (4b-3), `docs/state.md`.

## Done when

- The five-task click table in the Outcome.
- UI tests where egui can be driven headless; otherwise intent-level tests
  for each panel action, plus the screenshots.
- Screenshots described in the PR (not in git). Gates green. Outcome
  filled.

## Outcome

*(worker fills in)*
