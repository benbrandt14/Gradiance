# Context-menu actions are data
Phase 3 · scripting P3 · "Data-driven context-menu actions (first cut
landed)… Next: pass the click point / selection into the action, and fold
the existing hard-coded `ui::context_menu` buttons into the same table as
the built-in seed." · status: ready

## Goal

1. A script action knows where it was invoked. When a `register-action`
   action runs from the context menu, the run can read the world-space click
   point, e.g. `(action-x)` / `(action-y)`, or a bound pair; your choice,
   documented. A `register-action` like "drop a ball here" works.
2. The context menu's built-in, **intent-emitting** commands come from the
   same action table, as built-in entries a script could also invoke by
   name. The menu renders the table instead of hand-wiring each button.

## Read first

- `crates/gradiance-ui/src/context_menu.rs`: the whole menu. Hard-coded
  buttons at ≈L71, 110, 360, 376, 400, 413, 596, 646, 663; the "Scripts"
  section ≈L690–700 renders `ScriptActions`.
- `crates/gradiance-script/src/bridge.rs` ≈L345–365: `ScriptActions`,
  `ScriptAction`, `upsert`; the `register-action` builtin; `ScriptInputs`.
- `crates/gradiance-ui/src/menu.rs`: `Panels::named`, the precedent for
  "one table, two surfaces" (View menu + `panel-*` verbs) and its no-drift
  test.
- `docs/orchestration.md` § Future intent 1: the UI will be replaced; menu
  and action tables are data a new shell reads. Keep `gradiance-ui` a
  consumer.

## Facts (orchestrator, 2026-11-13, main `93c9a76`)

- `ScriptActions` lives in `gradiance-script` and stores `{label,
  source}`. An invoked action submits its source through `ScriptInputs`.
  Nothing carries the click point today.
- The context menu's built-in buttons are a mix:
  - some emit intents (delete node/joint, random colours);
  - some open panels ("Properties…", "Depth panel…", "Scene & lighting
    settings…");
  - some write settings ("Restore gravity");
  - some are selection transitions ("Configure joint…").
  Not all belong in a script-invocable table. **Classify each** in the
  Outcome: which are folded, which stay UI-local, and why.
- CLAUDE.md scripting rule 2: the click point and selection live above the
  script layer. The click point can be **passed in** with the invocation
  (the UI publishes it, like the `PanelStates` mirror). The script must not
  reach up for it. Selection stays out (rule 2; indexed verbs instead).

## Approach

1. Invocation context: the UI submits an action together with its click
   point. The script run exposes it only for that run (one snapshot per
   run; a REPL line has none, so the reads return NaN or error, as
   documented).
2. A built-in action seed: the folded commands become table entries whose
   bodies are verb calls (e.g. `(delete-joint i)`), or a typed built-in
   variant if a command has no verb yet. Don't add verbs beyond what
   folding needs; record any missing verb as retained (state.md has a
   retained list; the orchestrator files it).
3. The menu renders the folded entries from the table. A test asserts the
   menu's entries and the table can't drift (the `Panels::named` pattern).

## Out of scope

Selection-scoped script verbs, new menu items, hot keys for actions, menu
styling, `docs/state.md`.

## Done when

- Integration tests: an action registered as "spawn a circle at the click
  point" invoked with a point spawns there; one undo reverts it; a REPL
  read of the click point is the documented "none". The no-drift test.
- A screenshot of the context menu after the change (via
  `tools/screenshot/`; right-click works there) is described in the PR, not
  committed, and its path reported.
- `docs/scripting.md` documents the click-point reads; roadmap P3 line
  updated. Gates green. Outcome filled, including the classification.

## Outcome

*(worker fills in)*
