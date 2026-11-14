# Script examples speak metres
Phase 3 · polish of landed scripting · from screenshot pass 2 and the 3-4
Outcome · status: ready

## Goal

Every example a user sees for the scripting surface works when typed as-is:
the bodies it spawns land on screen at a sensible size. Today the spawn verbs
take **metres** (1 unit = 100 px at start-up zoom), but two user-facing
places still show pixel-era numbers:

1. The console input's placeholder (`crates/gradiance-ui/src/console.rs`
   ≈L285, `.hint_text("(spawn-box 0 200 40 40)")`) spawns a 40 m box 200 m
   up, far off-screen.
2. `docs/scripting.md` examples (e.g. `spawn-ground 0 -200 0`, 20-unit
   boxes), found by the 3-4 worker: a curl call using them put the body
   off-screen.

## Read first

- `docs/scripting.md` (all of it: every fenced example).
- `crates/gradiance-script/src/registry.rs`: each `OpSpec`'s `signature`
  and `doc` (the console's reference panel shows these, so they are
  user-facing too).
- `crates/gradiance-script/src/bridge.rs`: the spawn verbs' unit handling
  (confirm metres before rewriting anything).
- Any `.scm` under the repo (`git ls-files '*.scm'`), plus `README.md`.

## Facts (orchestrator, 2026-11-14, main `294a47d`)

- Start-up view: world (0,0) is screen centre; 100 px per metre; the ground
  a default scene spawns sits near y ≈ −2..−3 m. *Unverified:* the exact
  ground height the docs should assume. Read it from the default scene.
- 2-6's harness note: "Spawn verbs take metres."

## Approach

1. Audit every user-facing example: the placeholder, `OpSpec` docs,
   `docs/scripting.md`, `.scm` files, README. List each one in the Outcome
   with old → new.
2. Rewrite them to metre values that land on screen (e.g. `(spawn-box 0 1
   0.4 0.4)`). Don't change any verb's behaviour or units.
3. A test that guards the placeholder and the `OpSpec` examples: e.g. run
   each registry example through the bridge headlessly and assert the
   spawned body is inside a ±6 m × ±4 m view box. If that is impractical
   for some ops, test the placeholder at least, and say why.
4. Screenshot (`tools/screenshot/`): the placeholder's example run in the
   console, the body visible. Keep it out of git; report the path.

## Out of scope

New verbs, unit-typed arguments (Phase 5), console styling, behaviour
changes, `docs/state.md`.

## Done when

- The audit table is in the Outcome; every example lands on screen.
- The guard test; gates green; screenshot described in the PR.

## Outcome

*(worker fills in)*
