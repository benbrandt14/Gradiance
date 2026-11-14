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

Four ways in, all funnelling through one queue (`ScriptInputs`) and one
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
- **A remote client.** `gradiance --remote` serves the op registry over
  JSON-RPC on localhost; each call is one run (see *Remote control*).
- **Tests.** Submit source to `ScriptInputs` and step a frame. A scene fixture
  becomes a few lines of lisp asserted against the real authored world (see
  `tests/it/scripting.rs`).

### The run budget, and all-or-nothing runs

Each run (a console line, a `--script` file, a hot reload) gets a
**wall-clock budget of 2 s** (`script::budget::SCRIPT_BUDGET`). A script that
runs past it — `(define (f) (f)) (f)`, a loop with no exit — is stopped and
reported in the console like any other error: `stopped after 2000 ms: budget
exceeded`. The editor keeps running and the next line evaluates normally.

A run is **all or nothing**, like its one undo batch: if it fails for any
reason (evaluation error, panic, budget), nothing it queued is applied — no
edits, `sim-set` writes, labels, actions, panel requests, `signal-set`
publishes or `defparam`/`defsignal` declarations. Other runs in the same frame
are unaffected. (`define`s the failed run made before it stopped do stay in the
VM — they are interpreter state, not scene state.)

The budget is a guard, not a scheduler: the VM checks it between bytecode
instructions, so one *native* builtin call (a `(range 0 n)` with a huge `n`)
finishes before the stop lands. A script can stall the editor for as long as
that one call takes, then fails. Memory is not limited.

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
(spawn-box-rot x y w h angle) ; spawn-box, rotated by angle radians
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

;; edit — sets of bodies (one list argument; Ctrl+G / Ctrl+Shift+G as ops)
(group (list 0 2 3))         ; wrap the listed bodies in one fresh group
(ungroup (list 0 2))         ; peel the listed bodies' outermost group
;; duplicates collapse; fewer than two distinct bodies is a quiet no-op
;; (as Ctrl+G with one body selected). A non-list, a non-index element or
;; an out-of-range index is a script error, and the run commits nothing.

;; config — tune the simulation (not undoable; the settings seam)
(sim-get "gravity.y")        ; read a SimSettings field by reflect-path
(sim-set "gravity.y" -5)     ; write one (any scalar field, by path; m/s², SI)
;; 0 and 0.0 are the same value; a bad path or a non-number is a script
;; error in the console, never a silent no-op. The next physics step sees it.

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
(action-x) (action-y)        ; where the running action was invoked (NaN otherwise)
(tool-count)                 ; points the running script tool collected (NaN otherwise)
(tool-x i) (tool-y i)        ; its i-th point, click order (NaN outside a tool run)

;; editor — extend the tool
(register-action label src)  ; add a labelled action to the context menu
(register-tool name n src)   ; add a palette tool: n clicks, then src runs once
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

### Actions: the click point and the built-in table

The context menu invokes an action **at the right-click point**: the UI passes
the world-space point in with the invocation (`ScriptInputs::invoke`), and the
run reads it with `(action-x)` / `(action-y)`, in the same world units the
`spawn-*` verbs take. The point is visible to that run only. Anything that is
not an invoked action (a REPL line, a `--script` file, a hot reload) reads
**NaN**, so a dual-use script tests it with `nan?`:

```scheme
(register-action "Drop a ball here"
  "(spawn-circle (action-x) (action-y) 12)")
(if (nan? (action-x)) "not invoked from the menu" (action-x))
```

The script does not reach up for the point (scripting rule 2): the UI owns the
click and hands it down. The selection is deliberately *not* passed; resolve
a target from the point instead, e.g. `(body-index-at (action-x) (action-y))`.

The same table (`gradiance_script::actions::ScriptActions`) also holds the
context menu's **built-in** entries, seeded by `actions::builtin_seed()` and
named by `actions::builtin` (`restore-gravity`, `top-down`). Their bodies are
ordinary verb calls, so a script does the same thing by evaluating the same
text; the menu renders them from the table, and a test pins that the menu
places every seeded entry and no other. `register-action` adds user entries
(shown under "Scripts") and never replaces a built-in. Menu commands that need
the selection or a verb that does not exist yet stay hand-wired; the list and
the reasons are in `docs/briefs/3-3-action-table.md`.

### Script tools

`(register-tool name points src)` adds a tool to the palette. It appears
after the built-ins, under a separator, with a shared icon and its name as
caption and hover text. Select it, and it collects `points` clicks (1 to 64)
the way the polygon tool does: the same snapping, the tangent snap from the
last point, a preview of the points and the polyline through them. Escape or
a right-click cancels. The last click runs `src` **once**; the run reads the
points with `(tool-count)`, `(tool-x i)` and `(tool-y i)`, in the units the
`spawn-*` verbs take. A two-click ramp:

```scheme
(register-tool "ramp" 2
  "(let* ((ax (tool-x 0)) (ay (tool-y 0))
          (bx (tool-x 1)) (by (tool-y 1))
          (dx (- bx ax)) (dy (- by ay)))
     (spawn-box-rot (/ (+ ax bx) 2) (/ (+ ay by) 2)
                    (sqrt (+ (* dx dx) (* dy dy))) 0.1
                    (angle (make-rectangular dx dy))))")
```

`spawn-box-rot` exists for this: `place` indexes the pre-run snapshot, so a
run cannot rotate a box it has just spawned (one snapshot per run). steel
has no two-argument `atan`; `(angle (make-rectangular dx dy))` is atan2.

Why it is shaped this way:

- **The VM runs once per gesture, never per frame.** A tool's `update` and
  `preview` run every frame, so they are Rust: one generic `ScriptTool`
  draft (`interaction::tools::script_tool`) behind every registered tool,
  driven by the same `run_draft_tool` as box, circle and polygon. Only the
  completed gesture enters the VM. One gesture is one run, so it is
  all-or-nothing and, for a single edit, one undo step.
- **No upward edge.** `gradiance-interaction` and `gradiance-script` may not
  depend on each other. They meet in `gradiance-core::script_tools`: the
  script layer publishes the `ScriptTools` table, the tool reads it (how many
  points) and sends `ScriptToolFired { tool, points }` on the last click, and
  `run_scripts` turns that message into the run (`ScriptInputs::invoke_tool`).
- **Selection is `ToolState::Script(i)`**, `i` an index into `ScriptTools`.
  Re-registering a name (a hot reload) updates the entry in place, and
  entries are never removed, so an index stays valid for the session.

Tools are not saved with the scene (like actions and labels): they live
while their script is loaded. In a run that is not a tool's, `(tool-count)`,
`(tool-x i)` and `(tool-y i)` read NaN; inside one, an index past the last
point is an error.

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
(register-action "Drop a ball"      "(spawn-circle (action-x) (action-y) 12)")
(register-action "Heavy gravity"    "(sim-set \"gravity.y\" -25)")
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

Group/ungroup follow the same rule with a **set** of indices: their natural
argument is a set, and a fixed-arity `(group i j)` would be an arbitrary
restriction. steel's variadic `RestArgs` is private, so the set is one list
argument — `(group (list 0 1 2))`. They emit exactly the `GroupIntent` /
`UngroupIntent` Ctrl+G / Ctrl+Shift+G emit, so a scripted group is the same
save file and the same single undo step as a hand-made one. Unlike the scalar
index verbs (where a bad index no-ops), a bad member of the set is a script
error: a set edit that silently dropped one member would author a different
set than the script says, with nothing in the scene to show it.

```scheme
(group (list 0 1))           ; 0 and 1 now select together
(group (list 0 1 2))         ; nest: {0 1} inside {0 1 2}
(ungroup (list 0 1 2))       ; peel the outer level; {0 1} survives
```

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

## Remote control

A process outside the app can drive Gradiance through **the same operation
registry** over Bevy's remote protocol (BRP: JSON-RPC 2.0 over HTTP). It is
the door an external UI or an MCP server binds to.

**Off by default.** Start the app with `gradiance --remote` (port 15702) or
`gradiance --remote 15800`. The server binds **127.0.0.1 only** and has no
authentication: the local machine is the boundary.

| Method | Params | Result |
|---|---|---|
| `gradiance.ops` | none | every op: `name`, `signature`, `doc`, `category`, `args` |
| `gradiance.op` | `{"name": "<op>", "args": [...]}` (`args` optional) | the op's value as JSON |
| `gradiance.query` | same | same, but only `query` ops are accepted |

```sh
curl -s localhost:15702 -d '{"jsonrpc":"2.0","id":1,"method":"gradiance.op",
  "params":{"name":"spawn-box","args":[0,1,1.6,1.2]}}'
# {"jsonrpc":"2.0","id":1,"result":"6f1c…"}   (the new body's handle)
```

**A call is one script run.** `gradiance.op` checks the name against the
catalog, renders each argument as a Scheme literal (numbers, strings,
booleans; arrays become `(list …)`; `null` and objects are refused), and
submits `(spawn-box 0 1 1.6 1.2)` to `ScriptInputs`, then runs it through
`run_scripts` like a console line: same snapshot rule, same 2 s budget, same
all-or-nothing, one batch of undoable commands, and a line in the console
log. The reply comes back at the end of the frame the run happened in (BRP
handles requests after `Last`); the run's intents become commands in the
next frame's `Update`, as a console line's do, so a client's next call reads
the write. Two calls are two runs (two snapshots); to compose edits
atomically, `register-action` a helper and call it instead.

**Errors** are JSON-RPC errors:

| Code | When | `message` |
|---|---|---|
| `-32602` (invalid params) | unknown op, bad params shape, an argument with no Scheme form, a non-query op sent to `gradiance.query` | what was wrong |
| `-32000` (script error) | the run failed (evaluation error, panic, budget) | the console's text, verbatim; `data.source` is the run's source |

**Writes stay seam-mediated.** Bevy's built-in world-mutation methods
(`world.spawn_entity`, `world.insert_components`, `world.mutate_components`,
`world.remove_components`, `world.despawn_entity`, `world.reparent_entities`,
`world.insert_resources`, `world.mutate_resources`, `world.remove_resources`,
`world.trigger_event`, `world.write_message`) are removed, as are the `+watch`
streams and `schedule.*`. The reflection **reads** stay: `rpc.discover`,
`world.query`, `world.get_components`, `world.list_components`,
`world.get_resources`, `world.list_resources`, `registry.schema` (reads are
total). With a renderer, Bevy also opens a render-world port (15703); its
method table is emptied, so it answers nothing.

## What's next (see the roadmap)

- **User tools from `.scm`** — **landed** (brief 3-5) as `register-tool`: a
  Rust-side draft collects the points with the built-in tools' driver and the
  script body runs once per gesture. See *Script tools* above.
- **Driver dataflow (sensors / modulators / actuators)** — named-signal bindings
  lowered to the allocation-free Tier-B kernel (`gradiance-kernel`); a
  sensor is a query, a modulator is a kernel, an actuator is a config/edit op.
  **Landed** as `gradiance-signal` — bindings, params, and computed signals all
  compile through `signal::compile` to a `Kernel` tape.
- **A run budget** for runaway authoring scripts — **landed** (a wall-clock
  budget per run; failed runs commit nothing). See *The run budget* above.
