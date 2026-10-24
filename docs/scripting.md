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

- **The console.** Press `` ` `` (backquote) to toggle the script dock — a
  lisp REPL with MATLAB-style keys: **Enter runs**, **Shift+Enter** inserts a
  newline, **↑/↓** walk the history filtered by the prefix you've typed. Each
  run's last value is echoed in the log and bound to **`ans`** for the next
  line. Highlighting and the **Reference** panel are driven by the live
  operation catalog, so what the dock advertises is exactly what the VM
  understands.
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
| **editor**    | `register-action`, `label`             | writes a **non-authored editor resource**      |

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
(spawn-box x y w h)          ; a box centred at (x, y) → its handle
(spawn-circle x y r)         ; a circle centred at (x, y) → its handle
(spawn-ground x y angle)     ; a fixed ground half-plane → its handle
(cut ax ay bx by width)      ; sever every body crossed by the stroke a→b
(delete i)                   ; delete the i-th body (id order, as body-x)
(undo) (redo)                ; walk the command stack — Edit ▸ Undo/Redo as ops

;; edit — relationships between bodies (b < 0 pins to the world)
(hinge a b x y)              ; revolute joint at world point (x, y) → its handle
(slider a b x y ax ay)       ; prismatic joint at (x, y) along world axis (ax, ay)
(spring a b stiffness damping) ; spring-damper strut between the two centres

;; edit — a body's authored properties (the inspector's fields as ops)
(place i x y angle)          ; move and rotate the i-th body
(set-friction i v)           ; Coulomb friction (both coefficients)
(set-restitution i v)        ; bounciness, 0 = dead, 1 = perfectly elastic
(set-density i v)            ; mass density (area x density = mass)
(set-static i on)            ; non-zero = static, 0 = dynamic
(scale i fx fy)              ; resize about the body's own centre, own axes
(merge a b)                  ; fuse into one CSG union; a survives
(delete-joint i)             ; remove the i-th joint

;; config — tune the simulation (not undoable; the settings seam)
(sim-get "gravity.y")        ; read a SimSettings field by reflect-path
(sim-set "gravity.y" -500)   ; write one (any scalar field, by path)

;; editor — the chrome, through the EditorState seam
(panel-show "properties")    ; open a panel — the same toggle the View menu drives
(panel-hide "console")       ; close one
(panel-toggle "plot")        ; flip one
(panel-open? "depth")        ; #t / #f — whether it is showing
;; names: outliner properties depth plot nodes console probe array optimizer settings

;; query — read the committed scene (reads are total)
(body-count)                 ; number of bodies
(body-x i) (body-y i) (body-rot i)   ; pose of the i-th body (id order)
(joint-count)                ; number of joints
(body-friction i) (body-restitution i) (body-density i)  ; read them back
(body-static? i)             ; 1 when static, 0 otherwise
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
(defparam name value min max); a tunable slider knob, published on the bus
(defsignal name expr)        ; a computed signal from an RPN expression over
                             ; other signals + `t`, e.g. "t sin amp *"
(label body name)            ; name a body in the workspace (viewport tag);
                             ; body is a spawn's return value (or ans)

;; meta — introspection
(ops)                        ; list every registered op name
(describe "cut")             ; its signature and doc, as text
```

Reads observe **last-committed** state: a script's own `spawn-*` calls are
pending intents that land *after* the run, so `(body-count)` within the same run
does not yet see them. This is deliberate — it is the last-frame-read discipline
the future driver dataflow uses.

### The workspace

Spawn verbs return an opaque **handle** (the body's stable id), so a scene
built from the REPL stays addressable, MATLAB-workspace style:

```scheme
(define ball (spawn-circle 0 200 12))
(label ball "ball")          ; the body now wears a "ball" tag in the viewport
(label ans "latest")         ; ans = the last run's value, handles included
```

Labels are unique by name (re-labelling rebinds), never persisted, and show
up in the context menu's pick list. The `StableId` underneath remains the
durable identity.

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

A script-computed value driving a body's color (bind *named* `touches` to the
body's fill in the **signal list** beside the node canvas —
`docs/signal-dataflow.md`):

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

1. **`crates/gradiance-script/src/registry.rs`** — add a `name::MY_VERB` constant and an `OpSpec`
   entry in the matching `*_specs()` helper (name, signature, doc, category,
   arity). The console picks it up automatically.
2. **`crates/gradiance-script/src/bridge.rs`** — register the builtin in the matching
   `register_*_verbs` function, under the same `name::MY_VERB` constant:
   - an **edit** verb `emit`s a reflected intent onto the op queue;
   - a **config** verb reads the settings mirror / queues a reflect-path write;
   - a **query** verb reads the `SceneView` snapshot and returns a number;
   - an **editor** verb queues an editor-state change.
3. **Edits only:** add a row to `edit_bindings()` in `bridge.rs`
   (op name + the intent type it emits). `ScriptPlugin` registers the bus
   writer from that table, and `tests/it/registry_validation.rs` fails if a
   catalog Edit op has no binding, an unregistered intent, or an intent
   missing from the reflection registry.
4. Add a test (a bridge-level bus check and/or an end-to-end `tests/it/scripting.rs`
   case). Keep `cargo fmt`, `cargo clippy --all-targets -D warnings`, and
   `cargo test` green — the validation test above already covers the
   catalog↔builtin drift cases.

### Joints: relationships, not just objects

A multibody sandbox is mostly about *how things are connected*, and until these
verbs a script could author bodies but not a single constraint. All three of the
engine's joint kinds are reachable:

```scheme
;; a three-link chain, hinged at the shared edges
(begin (spawn-box 0 0 20 6) (spawn-box 30 0 20 6) (spawn-box 60 0 20 6))
(begin (hinge 0 1 15 0) (hinge 1 2 45 0))
```

Two things the verbs handle for you, because getting them wrong is subtle:

- **Anchors are local.** A joint stores its anchor in each body's own frame and
  records both bodies' rotations at creation (`rest_rot_*`) — welds hold that
  relative angle, sliders lock rotation to it, hinge limits measure from it.
  You pass a **world** point; the conversion happens in one place, the same one
  `interaction::tools::connector_tool` uses. Skipping it makes joints between
  rotated bodies snap violently at spawn.
- **`b < 0` is a world pin**, matching what the tools produce when you click
  where only one body sits. A *first* index that does not resolve emits nothing;
  a bad *second* index degrades to a world pin, so a typo is visible in the
  scene rather than silently dropped. A body hinged to itself becomes a world
  pin too.

Unlike the strut tool, `(spring …)` takes stiffness explicitly rather than
sizing it from the connected mass: a fixture whose stiffness depends on a mass
heuristic stops being reproducible the moment the shape changes.

There is no `weld` verb because there is no weld *joint* — in this engine
welding is `merge` (one CSG body) or make-static, not a constraint.

### Properties: reads and writes name the same thing

Every `set-*` verb has a matching `body-*` read, deliberately. Reads are total,
so a script can inspect a value it did not author and decide from it:

```scheme
;; make everything slippery except what is already slippery
(if (> (body-friction 0) 0.2) (set-friction 0 0.05) 0)
```

Three properties of the write side worth knowing:

- **One undo step, same as a hand edit.** They go through `PropertyEditIntent`,
  the seam the inspector's `precise_drag` rows commit through, so a scripted
  change and a dragged one are indistinguishable in the save file and the stack.
- **A redundant set is not a step.** Setting a value to what it already holds
  emits nothing, so a loop that normalises a scene does not bury the undo stack
  in empty entries. (`place` has the same guard.)
- **⚠ Within one run, every read sees the pre-run value.** Combined with the
  guard above this has a surprising consequence:

  ```scheme
  (begin (set-static 0 1) (set-static 0 0))   ; leaves the body STATIC
  ```

  Both calls read the same snapshot, so the second sees `old == new` and is
  suppressed. Writes to *different* properties in one run compose fine — each
  reads a field the other does not touch — but two writes to the **same**
  property need two runs. This is the same one-snapshot rule `(delete i)` has;
  it is just easier to trip over here.
- **A read of a missing body is NaN**, not zero — zero would read as a real
  measurement of a frictionless body. NaN compares false against everything, so
  a guard like the one above simply does not fire.

`(scale i fx fy)` works along the body's **own** axes about its own centre, so
"twice as wide" means along the body's width and a box stays a box. A
global-axis scale of a rotated box would shear it into a polygon — which
`geometry::scale` handles correctly, but it is not what the caller meant. Zero
and negative factors are rejected: zero is unrecoverable and a negative mirrors,
and neither is a resize.

`(set-friction …)` moves both the static and dynamic coefficients together,
which is what the inspector does: authoring two numbers that are almost always
equal is friction the UI deliberately does not expose.

### Why `(delete i)` and not `(delete-selection)`

The Edit menu's delete acts on the selection. A verb cannot: `Selection` lives
in `gradiance-interaction`, which sits **above** `gradiance-script` in the layer
graph, so the script layer cannot read it — the same asymmetry `panel-open?`
resolves with a mirror.

A mirror would be the wrong answer here, though. A script that deletes
"whatever happens to be selected" depends on invisible state and is not
reproducible, which is exactly what you do not want from a `.scm` fixture. So
edits are **indexed**, sharing the `i` vocabulary the query verbs already use
(`body-x i`, `body-y i`). The index is resolved against the run's snapshot, so
reads and edits compose within one script:

```scheme
(when (> (body-count) 0) (delete 0))   ; pop the first body
```

The snapshot is taken once per **run**, not per call — see the ⚠ note under
Properties for the case where that matters.

Group/ungroup are deliberately absent: their natural argument *is* a set, and a
fixed-arity `(group i j)` would be an arbitrary restriction rather than the op.
They wait for a list-shaped argument convention.

### Panels are registered ops

`panel-show` and friends resolve against **one** table — `Panels::named` in
`crates/gradiance-ui/src/panels.rs` — which is the same table the View menu
renders. Adding a panel is one row there and it appears in both, so the menu
and the API cannot drift; a unit test asserts every registry name has a menu
label.

The verbs add no mutation path. A verb queues a `PanelRequest`; the UI applies
it through `PanelToggle::set_open`, which is exactly what a menu click does.
The read direction (`panel-open?`) crosses the layer boundary as a mirror —
panel state lives in `gradiance-ui`, which sits *above* `gradiance-script`, so
the UI publishes `PanelStates` each frame rather than the script layer reaching
up.

`steel` may be declared only by `crates/gradiance-script` and `egui` only by
`crates/gradiance-ui` — the package graph enforces both, and
`tests/boundaries.rs` re-checks the manifests and the source text.

## What's next (see the roadmap)

- **User tools from `.scm`** — a scripted tool as a `ToolContext → (preview,
  commit-intent)` closure, reusing the same driver the built-in tools use.
- **Driver dataflow (sensors / modulators / actuators)** — named-signal bindings
  lowered to the allocation-free Tier-B kernel (`gradiance-kernel`); a
  sensor is a query, a modulator is a kernel, an actuator is a config/edit op.
  **Landed** as `gradiance-signal` — bindings, params, and computed signals all
  compile through `signal::compile` to a `Kernel` tape.
- **A fuel/step budget** for runaway authoring scripts.
