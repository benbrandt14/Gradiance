# Scripting Gradiance

Gradiance embeds a Lisp (Scheme, via [`steel`](https://github.com/mattwparas/steel))
as a **governed control plane**: scripts author the scene, tune the simulation,
query geometry, and extend the editor's menus — all through the *same* seams the
tools and UI use, never a bypass around them. This is the user-facing companion
to the design record in [`script-lisp-decision.md`](script-lisp-decision.md);
read that for the *why*.

> The scripting VM is a first-class, always-on part of the tool, but it only
> ever runs on the **authoring (cold) path** — never the per-frame loop. That is
> what makes an always-on Scheme engine free at runtime (see the two-tier rule
> in the decision doc).

## Running scripts

Three ways in, all funnelling through one queue (`ScriptInputs`) and one
exclusive doorway (`run_scripts`, which dispatches before the command stack, so
**one run = one batch of undoable commands**):

- **The console.** Press `` ` `` (backquote) to toggle the *Script Console*: a
  lisp editor with syntax highlighting, completion, an output log, and a
  **Reference** panel — all driven by the live operation catalog, so what it
  advertises is exactly what the VM understands. Type an expression and hit
  **▶ Run**.
- **A file at startup.** `gradiance --script scene.scm` reads a `.scm` file and
  runs it on the first frame — the natural place for scene setup, helper
  definitions, or `register-action` calls. Repeat `--script` for several files.
  Missing files warn and are skipped.
- **Tests.** Submit source to `ScriptInputs` and step a frame. A scene fixture
  becomes a few lines of lisp asserted against the real authored world (see
  `tests/it/scripting.rs`).

### Hot reload

Every `--script` file is watched while the app runs: **save the file and it
re-runs** (~0.5 s poll), no restart. Write files to converge under re-runs:

- **Definitions converge.** `register-action` (and future `register-*` verbs)
  *replace by name*, so a reloaded file updates your actions in place instead
  of duplicating them. Prefer registrations and helper `define`s in watched
  files.
- **Edits re-apply.** Spawn/cut/`sim-set` calls run again as ordinary new
  undoable commands — a reloaded scene-builder script will spawn a second
  copy of its scene. Either keep world-building out of watched files after the
  first run, or make it self-cleaning. Undo (`Ctrl+Z`) reverts a reload's
  batch like any other edit.

A file that is missing at launch (warned and skipped) starts running as soon
as it appears on disk.

## The governance model (why scripts can't break the rules)

Every operation belongs to one **category**, and each category routes through
exactly one sanctioned seam:

| Category      | Verbs (examples)                       | Seam it uses                                  |
|---------------|----------------------------------------|-----------------------------------------------|
| **edit**      | `spawn-box`, `spawn-circle`, `spawn-ground`, `cut` | emits an undoable **intent** (the command choke point) |
| **config**    | `sim-set`                              | writes a **settings resource** (the invariant-#4 seam) |
| **query**     | `body-count`, `body-x`, `count-at`, `sim-get`, … | **reads** a per-run snapshot — mutates nothing |
| **editor**    | `register-action`                      | writes a **non-authored editor resource**      |

**Reads are total; writes are seam-mediated.** A builtin physically cannot hold
`&mut World` (a `steel` constraint that happens to *be* the architecture), so an
edit verb can only emit intent *data*, and a query verb can only read a snapshot
taken before the run. There is no path by which a script mutates an authored
component directly — the same guarantee the tools and UI live under.

## Verb reference

The authoritative list is the console's **Reference** panel (and `(ops)` /
`(describe "name")` from inside a script). At time of writing:

```scheme
;; edit — author the scene (undoable)
(spawn-box x y w h)          ; a box centred at (x, y)
(spawn-circle x y r)         ; a circle centred at (x, y)
(spawn-ground x y angle)     ; a fixed ground half-plane through (x, y), tilted
(cut ax ay bx by width)      ; sever every body crossed by the stroke a→b

;; config — tune the simulation (not undoable; the settings seam)
(sim-get "gravity.y")        ; read a SimSettings field by reflect-path
(sim-set "gravity.y" -500)   ; write one (any scalar field, by path)

;; query — read the committed scene (reads are total)
(body-count)                 ; number of bodies
(body-x i) (body-y i) (body-rot i)   ; pose of the i-th body (id order)
(count-at x y)               ; how many bodies' shapes contain the point
(nearest-at x y)             ; index of the nearest body centre (-1 if none)
(nearest-dist x y)           ; distance to the nearest body centre (-1 if none)
(body-index-at x y)          ; index of a body containing the point (-1 if none)
(touch-count i)              ; how many bodies the i-th body is touching
(signal-get name)            ; current value of a named bus signal (NaN if unset)

;; editor — extend the tool
(register-action label src)  ; add a labelled action to the context menu
(signal-set name value)      ; publish a named value on the signal bus
                             ; (drives color/plot bindings — docs/signal-dataflow.md)

;; meta — introspection
(ops)                        ; list every registered op name
(describe "cut")             ; its signature and doc, as text
```

Reads observe **last-committed** state: a script's own `spawn-*` calls are
pending intents that land *after* the run, so `(body-count)` within the same run
does not yet see them. This is deliberate — it is the last-frame-read discipline
the future driver dataflow uses.

## Examples

A floor with a stack of boxes:

```scheme
(spawn-ground 0 -200 0)
(let loop ((i 0))
  (when (< i 5)
    (spawn-box 0 (* i 30) 20 20)
    (loop (+ i 1))))
```

Extend the right-click menu from an init script (`--script init.scm`):

```scheme
(register-action "Drop a ball"      "(spawn-circle 0 200 12)")
(register-action "Heavy gravity"    "(sim-set \"gravity.y\" -2500)")
(register-action "Fill a shelf"
  "(let loop ((i 0)) (when (< i 8) (spawn-box (* i 30) 0 24 24) (loop (+ i 1))))")
```

A script-computed value driving a body's color (bind *named* `touches` to
the body's fill in the **Signals** window — `docs/signal-dataflow.md`):

```scheme
(signal-set "touches" (touch-count 0))
```

Reads driving edits (a marker under every existing body):

```scheme
(let loop ((i 0))
  (when (< i (body-count))
    (spawn-circle (body-x i) (- (body-y i) 40) 4)
    (loop (+ i 1))))
```

## Adding a verb (contributor recipe)

The catalog and the steel registration are kept in lockstep by a shared name
constant, so a new verb touches three (edits: four) places:

1. **`src/script/registry.rs`** — add a `name::MY_VERB` constant and an `OpSpec`
   entry in the matching `*_specs()` helper (name, signature, doc, category,
   arity). The console picks it up automatically.
2. **`src/script/bridge.rs`** — register the builtin in the matching
   `register_*_verbs` function, under the same `name::MY_VERB` constant:
   - an **edit** verb `emit`s a reflected intent onto the op queue;
   - a **config** verb reads the settings mirror / queues a reflect-path write;
   - a **query** verb reads the `SceneView` snapshot and returns a number;
   - an **editor** verb queues an editor-state change.
3. **Edits only:** add a row to `edit_bindings()` in `src/script/bridge.rs`
   (op name + the intent type it emits). `ScriptPlugin` registers the bus
   writer from that table, and `tests/it/registry_validation.rs` fails if a
   catalog Edit op has no binding, an unregistered intent, or an intent
   missing from the reflection registry.
4. Add a test (a bridge-level bus check and/or an end-to-end `tests/it/scripting.rs`
   case). Keep `cargo fmt`, `cargo clippy --all-targets -D warnings`, and
   `cargo test` green — the validation test above already covers the
   catalog↔builtin drift cases.

`steel` may be imported only in `src/script/{bridge,reflect_bridge}` and `egui`
only in `src/ui/` — `tests/boundaries.rs` enforces both.

## What's next (see the roadmap)

- **User tools from `.scm`** — a scripted tool as a `ToolContext → (preview,
  commit-intent)` closure, reusing the same driver the built-in tools use.
- **Driver dataflow (sensors / modulators / actuators)** — named-signal bindings
  lowered to the allocation-free Tier-B kernel (`src/script/kernel.rs`); a
  sensor is a query, a modulator is a kernel, an actuator is a config/edit op.
- **A fuel/step budget** for runaway authoring scripts.
