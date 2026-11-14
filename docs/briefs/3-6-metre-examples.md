# Script examples speak metres
Phase 3 · polish of landed scripting · from screenshot pass 2 and the 3-4
Outcome · status: done (PR open)

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

**Landed.** Every user-facing example now uses metres and lands inside the
start-up view. No verb's behaviour or units changed.

Units confirmed before rewriting: `bridge::body_record` puts `x, y` straight
into the authored `PosRot` and `w, h, r` straight into `ShapeDef`, which are
SI. `units::world` is the only px seam. **No default ground:** an empty
start-up scene has no bodies (the `Startup` systems spawn none), so the
"ground near −2..−3 m" fact describes a user's scene, not a default. The docs
now spawn their own ground at `y = −2.5`.

### Audit (old → new)

| Where | Old | New |
|---|---|---|
| `console.rs` placeholder (now `ui::console::PLACEHOLDER`) | `(spawn-box 0 200 40 40)` | `(spawn-box 0 1 0.4 0.4)` |
| `registry.rs` `OpSpec` docs: spawn-box, spawn-circle, spawn-ground, spawn-box-rot, cut | no units | say "in metres" (signatures unchanged; they hold no numbers) |
| scripting.md, verb reference | no units | a line before the block: metres, 1 m = 100 px, view ≈ ±6 × ±3.5 m, radians |
| scripting.md, action point prose | "same world units" | "in metres" |
| scripting.md, "Drop a ball here" | `(spawn-circle (action-x) (action-y) 12)` | `… 0.12)` |
| scripting.md, workspace | `(spawn-circle 0 200 12)` | `(spawn-circle 0 2 0.12)` |
| scripting.md, floor + stack | `(spawn-ground 0 -200 0)`, `(spawn-box 0 (* i 30) 20 20)` | `(spawn-ground 0 -2.5 0)`, `(spawn-box 0 (- (* i 0.3) 2.3) 0.25 0.25)` |
| scripting.md, init-script "Drop a ball" | `… 12)` | `… 0.12)` |
| scripting.md, "Fill a shelf" | `(spawn-box (* i 30) 0 24 24)` | `(spawn-box (* i 0.3) 0 0.24 0.24)` |
| scripting.md, marker under each body | `(- (body-y i) 40) 4` | `(- (body-y i) 0.4) 0.05` |
| scripting.md, three-link chain | boxes `20×6` at 0/30/60, hinges at 15, 45 | boxes `0.3×0.08` at 0/0.3/0.6, hinges at 0.15, 0.45 (now the links really share edges, as the comment says) |
| Already metres, unchanged | ramp tool (`0.1` thick), remote `curl` (`[0,1,1.6,1.2]`), `remote.rs` doc (`(spawn-box 0 2 1 1)`), `sim-set` values (m/s²) | — |
| `.scm` files, README | none tracked; README only names `--script my-scene.scm` | — |

### Guard test

`tests/it/script_examples.rs`:

- `the_console_placeholder_lands_on_screen` runs `ui::console::PLACEHOLDER`
  (the widget renders that same const) and asserts one body inside ±6 m × ±4 m.
- `every_scripting_doc_example_lands_on_screen` pulls every fenced `scheme`
  block out of `docs/scripting.md` (`include_str!`) and skips only the verb
  reference (signatures). It runs each block in a fresh paused app seeded with
  three on-screen boxes, so index-based examples have targets. It then invokes
  every registered action at the origin and every registered tool with points
  in view. It asserts each run succeeded (a failed run commits nothing, so
  errors would let the check pass vacuously), that every body's rotated AABB
  is in the box with a smallest side ≥ 0.05 m, and that a ground's surface
  point is in view. It also asserts at least 10 examples and 10 spawned bodies.
- Checked against the old text: with main's `scripting.md` and the old
  placeholder, both tests fail ("a Box { width: 40.0 … } at [0, 200] …
  outside ±[6, 4] m"; "a Circle { radius: 12.0 } …").
- The `OpSpec`s have no numeric examples (only signatures), so there is
  nothing of theirs to run. The test covers the doc examples instead.

### Screenshot

Not in git. `--script` with `(spawn-ground 0 -2.5 0)`, `(panel-show
"console")` and the placeholder line. The console shows the placeholder as
hint text and the run in its log. The 0.4 m box sits on the ground near the
view centre. Typing into the console did not arrive (the known no-focus
limitation), so the placeholder ran from the startup script.

### Follow-ups (not done)

- `tests/it/scripting.rs` (and some bridge unit tests) still use pixel-era
  numbers (`(spawn-box 0 0 40 20)`, `(cut -40 0 40 0 2)`). They pass and no
  user sees them, but they read as if those were sensible sizes.
- The placeholder box falls out of view in an empty scene while playing, because
  there is no default ground. A start-up ground, or a hint that spawns one, is
  a product decision.
