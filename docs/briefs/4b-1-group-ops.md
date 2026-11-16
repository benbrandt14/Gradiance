# Groups as editing layers: the op surface
Phase 4b · owner direction 2026-11-16 · "ensure that a layered editing
workflow (in the form of groups) is clear and efficient. It should be simple
to quickly edit the parameters of a given group (ie color, but could be
anything) or to distribute them in certain bounds. There should be various
filtering to select among groups or create/decompose groups to suit this
workflow." · status: ready

## Goal

Groups become the unit of bulk editing. This brief builds the **model and
op surface**. The UI is brief 4b-2, which consumes it.

1. **Groups are first-class.** A group has a stable id, an optional
   **name**, and a list of members. Groups are listable and named, and
   their members are readable from scripts and the UI.
2. **Group-scoped edits.** Set any body property on every member of a group
   in one undo step: colour, density, friction, restitution, gravity scale,
   depth, sensor, field and so on, i.e. every `PropertyValue` that makes
   sense to broadcast.
3. **Distribute within bounds.** Spread one property across a group's
   members between `lo` and `hi`, in one undo step. Modes:
   - linear in member order, by a chosen sort key (x, y, size, current
     value);
   - evenly random with a seed;
   - for colour: along a palette or hue range.
4. **Filters.** A query returns the bodies matching predicates, composably:
   kind/shape, group, ungrouped, colour (palette slot), property range,
   depth band, label. A filter result can become a new group, or a group can
   be **decomposed** by a key: split by colour, by kind, by connectivity
   (merge islands), or dissolved.

## Read first

- `crates/gradiance-domain/src/group.rs`: `SelectionGroup` is a stack of
  anonymous `u32` ids, outermost last. Selection expands by outermost.
- `crates/gradiance-command/src/property.rs`: `PropertyValue`,
  `PropertyChange`, and `PropertyEditIntent` (already multi-body: one intent,
  many changes).
- `crates/gradiance-command/src/intent.rs`: `GroupIntent`, `UngroupIntent`;
  brief 3-2's Outcome (group/ungroup verbs, list arguments).
- `crates/gradiance-domain/src/appearance.rs`: `PALETTE`, `random_fills`
  (4-3).
- CLAUDE.md scripting rules. Rule 2 forbids selection-reading verbs, but a
  **group is authored scene state**, not selection, so group-indexed verbs
  are fine. Filters return index lists. The UI turns them into selections.
- `docs/scripting.md` § adding a verb; `docs/orchestration.md` § Future
  intent 1 (op surface first; a new shell must be able to drive all of
  this).

## Facts (orchestrator, 2026-11-16, main `7b22e7a`)

- **The save format and UI may change freely** (owner, 2026-11-16: "this is
  all experimental and not in active use"). Bump `FORMAT_VERSION` if needed,
  reject older files, and regenerate goldens deliberately. No migration
  code needed.
- Group names have no home today. *Unverified:* the best place for them:
  - a scene-content resource `GroupNames { id → name }` (saved, undoable via
    a command or the settings seam);
  - a component on a group entity.
  Pick one and say why. Group identity must stay stable across save/load
  and undo, so ids can't be renumbered.
- `set-color i r g b` was retained in 3-3 for lack of `Appearance` in the
  script snapshot. It is now in scope: put `Appearance` in the snapshot.
- A group edit is **one** `PropertyEditIntent` with one change per member,
  not N intents. Distribute is the same, with computed per-member values.
  Prefer pure functions in `gradiance-domain` for the value computation
  (`random_fills` is the precedent), so tests need no app.

## Approach

1. **Data:** group names; a pure `GroupIndex` view (groups, members, nesting)
   computed from components. Where it lives (domain or scene) must respect
   the DAG.
2. **Pure functions:** `distribute(values_by_member, lo, hi, mode, key)`, a
   filter predicate type, and decompose keys. Unit-test them heavily.
3. **Intents/commands:** reuse `PropertyEditIntent` for edits and
   distribute. Add an intent for naming, and for "group from list" or
   "decompose" only if `GroupIntent`/`UngroupIntent` can't express them in
   one undo step.
4. **Verbs** with `OpSpec` rows. Indicative names; choose a coherent family:
   - `(groups)`, `(group-name g)`, `(name-group g "wheels")`,
     `(group-members g)`;
   - `(group-set g "friction" 0.2)`, `(group-distribute g "density" 0.5 5
     "linear" "x")`;
   - `(bodies-where …)` returning indices, `(group-from (list …))`,
     `(decompose g "color")`;
   - `(set-color i r g b)`.
5. A "property name" table that verbs, the 4b-2 panel and the remote surface
   all share: one table, several surfaces.

## Out of scope

The Groups panel and any egui (4b-2), physical default scaling (4b-4),
palette redesign (4b-3), `docs/state.md`.

## Done when

- Unit tests for distribute (bounds respected, endpoints hit for linear,
  seeded random reproducible, colour along palette/hue), filters and
  decompose.
- Integration tests:
  - name a group, save, load, and it is still named;
  - a group-set and a distribute are each one undo step that restores
    every member exactly;
  - decompose by colour yields one group per colour;
  - a filter result → `group-from` → `group-members` round-trips;
  - `set-color` is reversible.
- Each new verb is documented in `docs/scripting.md` with an example; the
  3-6 guard runs every doc example, so make them metre-sized.
- Gates green. Outcome filled.

## Outcome

*(worker fills in)*
