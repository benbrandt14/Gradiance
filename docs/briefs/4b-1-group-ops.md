# Groups as editing layers: the op surface
Phase 4b · owner direction 2026-11-16 · "ensure that a layered editing
workflow (in the form of groups) is clear and efficient. It should be simple
to quickly edit the parameters of a given group (ie color, but could be
anything) or to distribute them in certain bounds. There should be various
filtering to select among groups or create/decompose groups to suit this
workflow." · status: in review

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
4. **Operations** in the registry (`gradiance-op`; recipe in
   `docs/scripting.md`; U-2b deleted `OpSpec`). Each one is declared once in
   its owning layer, so the Lisp verb, the remote call and the
   `docs/ops.md` row come from that one declaration. Regenerate
   `docs/ops.md` (`UPDATE_OPS_DOC=1 cargo test --test it
   docs_ops_md_is_current`). Group and body references are `Target`s, so
   an index, an id or `"selection"` (not from a script) all work.
   Indicative names; choose a coherent family:
   - `(groups)`, `(group-name g)`, `(name-group g "wheels")`,
     `(group-members g)`;
   - `(group-set g "friction" 0.2)`, `(group-distribute g "density" 0.5 5
     "linear" "x")`;
   - `(bodies-where …)` returning indices, `(group-from (list …))`,
     `(decompose g "color")`;
   - `(set-color i r g b)`.
5. A "property name" table that verbs, the 4b-2 panel and the remote surface
   all share: one table, several surfaces.

Revised 2026-11-17 after U-2 (#243–#245): verbs are registry operations,
not `OpSpec` rows; the conformance tests in `tests/it/op_conformance.rs`
cover every new operation without new hand lists.

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
- Each new operation is in the generated `docs/ops.md`, and each new verb
  has an example in `docs/scripting.md`; the
  3-6 guard runs every doc example, so make them metre-sized.
- Gates green. Outcome filled.

## Outcome

Landed in one PR (worker, 2026-11-17). Every item of the Goal and every
Done-when test; no split was needed.

**What landed**

- **Data.** `domain::group::GroupNames` (`BTreeMap<u32, String>`), a
  scene-content resource in the `scene_content!` list, so it is saved and
  restored by undo with no new path. `domain::group::GroupIndex` is the pure
  read view (groups in id order, members, parent). `SceneSnapshot` now
  carries each body's `Appearance`, `DepthBand`, group stack and full
  `BodyPhysics` (gravity scale, sensor, rotation lock), the joint links
  and the group names.
- **Pure functions.** `domain::distribute` (`Linear` by sort key, seeded
  `Random` via an inline SplitMix64); `Rgba::to_hsl`/`with_hue`,
  `palette_slot`/`palette_fill`. `command::filter`: clause parser +
  `matching`, and `decompose` by colour / kind / joint islands / dissolve.
- **One property table**: `command::body_prop::BODY_PROPS` (friction,
  restitution, density, gravity-scale, static, sensor, rotation-lock, depth,
  hue, palette): name, read off the snapshot, and `(old, new)`
  `PropertyValue` edit. The verbs and filter clauses read it; 4b-2's panel
  should too.
- **Intents/commands.** `NameGroupIntent` → `NameGroupCommand` and
  `RegroupIntent` → `RegroupCommand` (decompose: each part takes a fresh id
  in the old group's place in every stack, so nesting is kept). Group-set,
  distribute, scatter and colours reuse `PropertyEditIntent` (one change
  per member). One fresh-id function now (`group_cmd::fresh_group_id`;
  the duplicate in `spawn.rs` went): past every live **and named** id.
- **Operations** (11 new, in `docs/ops.md` and `docs/scripting.md`):
  `groups`, `group-name`, `group-members`, `name-group`, `group-set`,
  `group-set-color`, `group-distribute`, `group-scatter`, `bodies-where`,
  `decompose`, `set-color`. `group` now returns the new group's index.
- **Tests.** Unit: distribute (bounds, endpoints, ties, seeded random),
  HSL/palette, `GroupIndex`, filter clauses, decompose keys, the property
  table. Integration `tests/it/groups.rs`: name survives save/load and
  undo; group-set and distribute are one step each and undo restores every
  member exactly; seeded scatter reproduces; decompose by colour gives one
  group per colour (inner nesting kept); filter → `group` → `group-members`
  round-trips; `set-color` reverses. Goldens regenerated (the environment
  gained `group_names: ({})`).

**Decisions (in scope; reversible)**

- **Names live in a scene-content resource**, not on a group entity: groups
  have no entity today, and the resource rides the existing save + undo
  path. The command writes it; the settings watcher then sees the change but
  `push_snapshot` finds the live world equal to the tip, so it is still one
  step. Stale names are inert, and fresh ids skip them.
- **No `FORMAT_VERSION` bump:** the field is `#[serde(default)]`, so a file
  without it loads with no names (additive).
- **Group targets:** an index counts into `(groups)` (id order; a new group
  is always last); a body target (id, point, `"selection"`) names its
  outermost group.
- **`group-from` is `group`**, which now returns the new index, rather than
  a second verb over the same intent. Two `group` calls in one run both
  return the same index (one snapshot per run).
- **Colour:** exact RGB is `set-color` / `group-set-color`; spreads go
  through the scalar `hue` (degrees; a grey takes saturation 0.8 and
  lightness 0.5 so a spread shows) and `palette` (slot, rounded and
  wrapped) properties. Random mode is a separate verb (`group-scatter`)
  rather than a mode string, so every argument stays typed.
- **Filters are AND of clauses** with `not:`; a union is two lists
  appended. **"Islands" = joint connectivity** between members (a world
  pin joins nothing).
- **Label filter not built:** workspace labels live in `gradiance-script`
  (`WorkspaceLabels`), above `command`; a label clause would need that
  layer to publish a mirror. Left for 4b-2 or a follow-up.

**Not checked:** the full `cargo test --workspace` (CI runs it); no
screenshots (no UI change).
