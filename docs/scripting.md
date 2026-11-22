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
| **config**    | `sim-set`, `camera-set`                | writes a **settings resource** (the invariant-#4 seam) |
| **query**     | `body-count`, `body-x`, `count-at`, `sim-get`, … | **reads** a per-run snapshot — mutates nothing |
| **editor**    | `register-action`, `label`             | writes a **non-authored editor resource**      |

**Reads are total; writes are seam-mediated.** A builtin physically cannot hold
`&mut World` (a `steel` constraint that happens to *be* the architecture), so an
edit verb can only emit intent *data*, and a query verb can only read a snapshot
taken before the run. There is no path by which a script mutates an authored
component directly — the same guarantee the tools and UI live under.

## Verb reference

The authoritative list is **`docs/ops.md`**, generated from the operation
registry (a test fails when it is stale), and the console's **Reference**
panel (and `(ops)` / `(describe "name")` from inside a script). At time of
writing:

Lengths and positions are in **metres** (1 m = 100 px at start-up zoom,
world (0, 0) at the view centre, so the start-up view spans about ±6 m
across and ±3.5 m up and down); angles are radians.

```scheme
;; edit — author the scene (undoable)
(spawn-box x y w h)          ; a box centred at (x, y) → its handle
(spawn-circle x y r)         ; a circle centred at (x, y) → its handle
(spawn-ground x y angle)     ; a fixed ground half-plane → its handle
(spawn-box-rot x y w h angle) ; spawn-box, rotated by angle radians
(cut ax ay bx by width)      ; cut every body the stroke a→b passes through
(delete i)                   ; delete the i-th body (id order, as body-x;
                             ; ids are minted in creation order)
(undo) (redo)                ; walk the command stack — Edit ▸ Undo/Redo as ops
(last-op)                    ; the operation that made the newest step: {id, params}
(adjust-last "w" 2.5)        ; set one of its params and run it again in place of
                             ; its step: still one undo step (field "step.0" for
                             ; a list element); an error when the newest step is
                             ; not one operation call's (a gesture, an undo, a run)

;; edit — relationships between bodies (b < 0 pins to the world)
(hinge a b x y)              ; revolute joint at world point (x, y) → its handle
(slider a b x y ax ay)       ; prismatic joint at (x, y) along world axis (ax, ay)
(spring a b hz ratio)         ; spring-damper strut between the two centres

;; edit — a chain of hinged capsule links (brief N-5; the chain tool's verb)
(chain (list x0 y0 x1 y1 …) link width a b pins)
                             ; lay links about link m long, width m wide, along
                             ; the path (the nearest whole count, ends exactly on
                             ; the path's ends); pins "free", "bodies" (start to
                             ; body a, end to body b; -1 names none) or "world"
                             ; (an end with no body is pinned to the world)
                             ; → the links' ids; one spawn-chain step

;; edit — a freehand stroke as one body (brief N-6; the sketch tool's verb)
(sketch (list x0 y0 x1 y1 …) width)
                             ; simplify the stroke (RDP) into at most 24 capsule
                             ; segments width m wide, joined as one SDF union
                             ; (a straight stroke is one capsule); a stroke that
                             ; ends near its start and encloses an area fills as
                             ; a polygon instead → its handle; one spawn-body step

;; edit — behavior nodes on a body (briefs N-3, N-8)
(thruster i x y angle thrust) ; push body i at world point (x, y) with thrust N
                             ; along the world angle (radians) → its handle;
                             ; the point and axis ride and turn with the body
(laser i x y angle target name)
                             ; a laser at world point (x, y) riding body i (-1:
                             ; free), aimed along the world angle; its beam
                             ; reflects off bodies; the bus value name reads 1
                             ; while the beam reaches body target (-1: none),
                             ; name.distance its path length (m) → its handle;
                             ; read them with (signal-get "name")

;; edit — a body's authored properties (the inspector's fields as ops)
(place i x y angle)          ; move and rotate the i-th body
(set-friction i v)           ; Coulomb friction (both coefficients)
(set-restitution i v)        ; bounciness, 0 = dead, 1 = perfectly elastic
(set-density i v)            ; mass density (area x density = mass)
(set-static i on)            ; non-zero = static, 0 = dynamic
(set-fluid i 2 [2])          ; a fluid of density 2 kg/m² (drag 2 /s): bodies
                             ; inside float or sink by density (brief N-7);
                             ; also makes it a static sensor; density 0 = off
(set-velocity i vx vy)       ; the velocity (m/s) the body starts a run with
(set-spin i w)               ; the spin (rad/s, CCW) it starts a run with;
                             ; applied when play starts from an edited
                             ; layout, not on resume (brief S-1)
(set-arrows i v m f)         ; show the body's velocity / momentum / force
                             ; arrows while it plays (1 = on; brief S-2b)
(scale i fx fy)              ; resize about the body's own centre, own axes
(mirror-h (list i …))        ; mirror left to right about the bodies' centre,
(mirror-v (list i …))        ; or top to bottom; joints and nodes follow
                             ; (brief N-9); one undo step each
(merge a b)                  ; fuse into one CSG union; a survives
(subtract a b)               ; cut b's shape out of a; b is consumed
(intersect a b)              ; keep only the overlap, on a; b is consumed
(xor a b)                    ; keep what is in exactly one of a and b, on a
(fillet-merge a b 0.25)      ; merge with a 0.25 m fillet where they meet; a survives
;; booleans: a hosts the analytic tree; a subtract/xor that splits a becomes
;; polygon pieces (like a severing cut); an empty or missing result does nothing
;; fillet-merge: the radius (metres) must be a finite number > 0, else a
;; script error and the run commits nothing. The blend also joins bodies
;; less than radius/2 apart, which a plain merge refuses as disconnected.
;; Fillets render as a curve from ~4 contour cells (shape extent / 96 each:
;; about 0.1 m on a 2 m body); smaller radii read as a chamfer.
(array-copies (list i …) n (list dx dy) rows (list cx cy))
                             ; n copies at (dx, dy) steps, plus rows more rows at
                             ; (cx, cy): the Ctrl-drag array as one undo step
(delete-joint i)             ; remove the i-th joint

;; edit — sets of bodies (one list argument; Ctrl+G / Ctrl+Shift+G as ops)
(group (list 0 2 3))         ; wrap the listed bodies in one fresh group
(ungroup (list 0 2))         ; peel the listed bodies' outermost group
;; duplicates collapse; fewer than two distinct bodies is a quiet no-op
;; (as Ctrl+G with one body selected). A non-list, a non-index element or
;; an out-of-range index is a script error, and the run commits nothing.
;; group returns the new group's index (groups are listed in id order and
;; a new group always lands last)

;; groups as editing layers — g is a group index (as in (groups)) or a body
;; (an id or {"at": [x, y]} from the remote surface: its outermost group)
(groups)                     ; every group's name, in group order ("" = unnamed)
(group-name g)               ; one group's name
(group-members g)            ; its members' body indices (any nesting depth)
(name-group g "wheels")      ; name it ("" removes the name) — undoable, saved
(group-set g "friction" 0.2) ; one property on every member — one undo step
(group-set-color g r g b)    ; every member's fill, each channel 0..1
(group-distribute g "density" 0.5 5 "x")  ; lo..hi in order of x (or y, size,
                             ; order, value) — first gets lo, last gets hi
(group-scatter g "hue" 0 300 7)           ; seeded random in lo..hi
(bodies-where (list "kind:circle" "ungrouped"))  ; indices matching every clause
(decompose g "color")        ; one group per colour (or "kind", "islands",
                             ; "dissolve") — one undo step
(set-color i r g b)          ; one body's fill, each channel 0..1
;; properties (the table in crates/gradiance-command/src/body_prop.rs):
;; friction restitution density gravity-scale static sensor rotation-lock
;; depth hue palette. Clauses: kind:<box|circle|polygon|rod|plane|csg>,
;; group:<g>, grouped, ungrouped, palette:<slot>, <property>:<lo>..<hi>,
;; <property>:<v>, not:<clause>.

;; config — tune the simulation (not undoable; the settings seam)
(sim-get "gravity.y")        ; read a SimSettings field by reflect-path
(sim-set "gravity.y" -5)     ; write one (any scalar field, by path; m/s², SI)
;; air drag and wind (brief N-4): drag_coefficient (Cd, 0 = off),
;; linear_drag (N s/m², 0 = off), air_density (default 1.2), wind.x, wind.y
(sim-set "drag_coefficient" 1) (sim-set "wind.x" 3)
;; 0 and 0.0 are the same value; a bad path or a non-number is a script
;; error in the console, never a silent no-op. The next physics step sees it.
;; camera feel — workstation config: saved with the scene, never undone
(camera-get "zoom_step")     ; zoom multiplier per scroll notch (0.5..0.995)
(camera-set "pan_speed" 900) ; arrow-key pan, screen px/s; also "orbit_speed_deg"
;; chrome preset — workstation config: never undone, not saved (brief U-7)
(chromes)                    ; ("blender" "blender-blue" "stock")
(chrome-apply "blender-blue") ; draw the panels with another preset
;; unit display — workstation config: never undone, not saved (brief 5-3).
;; Display only: every verb still reads and writes SI (metres, radians).
(units-shown)                ; ("deg" "m") -- how fields show angles, lengths
(display-units "rad" "cm")   ; angles: deg rad; lengths: m cm mm

;; editor — the chrome, through the EditorState seam (panel.* operations)
(panel-show "properties")    ; open a panel — the same toggle the View menu drives
(panel-hide "console")       ; close one
(panel-toggle "plot")        ; flip one
(panel-open? "depth")        ; #t / #f — whether it is showing
(panels)                     ; every panel: name and whether it is open
;; names: outliner groups properties depth plot nodes console probe array optimizer
;;        settings review search status
;; an unknown name is a script error that lists these
(panel-swap "outliner" "console") ; a dock tile switches its editor in place (brief U-10)
;; workspaces — saved dock layouts; workstation config: saved with the scene,
;; never undone (brief U-10)
(workspace-select "Scripting") ; presets: Layout Scripting Signals; or a saved one
(layout-save "Mine")         ; save the docks (editors, splits, sizes) and make it active
(layout-apply)               ; put the docks back to the active workspace's saved layout
(workspaces)                 ; every tab: name, preset?, active?
(workspace-delete "Mine")    ; a saved one; presets cannot be deleted

;; tools — one operation per tool (brief U-9), the same pick the palette makes
(box-tool)                   ; also select-tool drag-tool circle-tool polygon-tool hinge-tool
                             ; prismatic-tool spring-tool rod-tool weld-tool ground-tool
                             ; cut-tool tracer-tool; editor seam, no undo step
(ui-search "del")            ; the ids F3 search lists for "del", best first

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

;; query — the physical quantity catalog (brief 5-2; SI values, live state)
(quantities)                 ; every quantity name, catalog order
(quantity-unit "speed")      ; its SI unit symbol: "m/s" ("" for a count)
(quantity "speed" i)         ; a quantity of the i-th body (NaN if out of range)
(quantity "joint-angle" j)   ; a joint-* quantity reads the j-th joint
(quantity-between "distance" a b) ; a two-body quantity
;; names: speed spin height pos-x contact-force contact-count kinetic-energy
;; momentum angular-momentum mass distance joint-length joint-angle joint-force
;; an unknown name, or a name read with the wrong verb, is a script error
;; that says what to write. The values are the ones the sensor ports and the
;; plot show (one table: crates/gradiance-domain/src/quantity.rs), read once
;; before the run like every other query.

;; units — numbers in the units you think in, converted to SI (brief 5-4)
(q 25 "cm")                  ; 0.25 -- a number in a unit, in SI
(q 90 "deg")                 ; 1.5708 (pi/2); units: m km cm mm um in ft kg g mg t
                             ; s ms min h rad mrad deg rev turn N mN kN MN J mJ kJ
                             ; Hz kHz rpm, and anything they compose: "km/h" "N m"
(convert 0.25 "m" "cm")      ; 25 -- between two units of one dimension
(convert 90 "km/h" "speed")  ; 25 -- a quantity name stands for its SI unit
(unit-of "km/h")             ; "m/s" -- what a unit measures, as quantity-unit says
;; an unknown unit, or a conversion between dimensions (convert 90 "deg" "m"),
;; is a script error. See "Units" below.

;; run & read — an experiment end to end (brief R-1; no undo steps but load)
(sim-play) (sim-pause)       ; the transport's Play / Pause (the Space key's path)
(sim-step 60)                ; pause, then advance exactly 60 fixed steps; returns
                             ; at once, the steps run over the next fixed ticks
(sim-time)                   ; simulated seconds on the physics clock
(scene-dump)                 ; the whole scene as JSON text: the record a save writes
(save-scene "run.ron")       ; write the scene (.ron; "" = the last path, else a dialog)
(load-scene "run.ron")       ; replace the scene with a file's: one undo step
(signal-history "t")         ; a signal's recorded samples: ((t v) ...), oldest first
(signal-export "run.csv" (list "t" "speed"))  ; t,t,speed rows, one per sample
                             ; of the first signal; returns the path

;; parts and the clipboard (brief R-3; insert and paste are one undo step)
(insert-scene "parts/arm.ron" (list))      ; add a file's objects, fresh ids, where saved
(insert-scene "parts/arm.ron" (list 4 2))  ; ... centred at (4, 2); returns the new ids
(copy-ron (list 0 1))        ; bodies 0 and 1 (+ joints between them) as scene text,
                             ; also put on the clipboard
(paste-ron text (list 0 3))  ; add scene text, fresh ids, centred at (0, 3)

;; editor — extend the tool
(register-action label src)  ; add a labelled action to the context menu
(register-tool name n src)   ; add a palette tool: n clicks, then src runs once
(signal-set name value)      ; publish a named value on the signal bus
                             ; (drives color/plot bindings — docs/signal-dataflow.md)
(defparam name value min max); a tunable slider knob, published on the bus
(defsignal name expr)        ; a computed signal from an RPN expression over
                             ; other signals + `t`, e.g. "t sin amp *"
(defkey name key [negative]) ; a key input: 1 while key is held in play,
                             ; -1 for negative (brief N-2), e.g. "E" "Q"
(bind name source sink [target] [of] [min] [max])
                             ; bind a bus signal (or, with of, a quantity
                             ; read off those subjects) to a sink: fill,
                             ; tracer, plot, motor-speed, motor-cap, field,
                             ; gravity, thrust on target (a joint for motor-*,
                             ; a node for thrust, else a body); brief Q-2
                             ; (bind "spin" "osc" "motor-speed" 0)
                             ; (bind "v" "speed" "plot" -1 (list 0))
(unbind name)                ; remove a binding; the authored value returns
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
run reads it with `(action-x)` / `(action-y)`, in metres, the units the
`spawn-*` verbs take. The point is visible to that run only. Anything that is
not an invoked action (a REPL line, a `--script` file, a hot reload) reads
**NaN**, so a dual-use script tests it with `nan?`:

```scheme
(register-action "Drop a ball here"
  "(spawn-circle (action-x) (action-y) 0.12)")
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
(define ball (spawn-circle 0 2 0.12))
(label ball "ball")          ; the body now wears a "ball" tag in the viewport
(label ans "latest")         ; ans = the last run's value, handles included
```

Labels are unique by name (re-labelling rebinds), never persisted, and show
up in the context menu's pick list. The `StableId` underneath remains the
durable identity.

### Run and read an experiment (brief R-1)

```scheme
(load-scene "tests/fixtures/scenes/pendulum.ron")
(sim-step 120)               ; two simulated seconds at 60 Hz
;; a later run, once (sim-time) has moved on by 2:
(signal-export "swing.csv" (list "t"))
(save-scene "swing.ron")
```

| Verb | Id | Seam | What it does |
|---|---|---|---|
| `(sim-play)` | `sim.play` | editor | Runs the simulation, as the transport's Play. Drops steps a `sim-step` still owes. |
| `(sim-pause)` | `sim.pause` | editor | Freezes it, as Pause. Drops owed steps. |
| `(sim-step n)` | `sim.step` | editor | Pauses, then advances exactly `n` fixed steps (1 to 216000) of the physics clock, each the fixed timestep times the sim speed, as a playing tick. Returns at once; the steps run one per fixed tick. Field forces, joint breaking, signal recording and tracer trails run while steps are owed, as in play. A second request adds to the steps owed. |
| `(sim-time)` | `sim.time` | query | Simulated seconds on the physics clock. Poll it to see a `sim-step` done. |
| `(scene-dump)` | `scene.dump` | query | The scene as JSON: the `SceneRecord` a save writes (bodies, joints with their anchors, groups, nodes, environment). A remote client gets the object, Lisp its text. |
| `(save-scene path)` | `scene.save` | editor | Writes the scene to `path` (must end in `.ron`). `""`: the last saved or opened path, else a dialog (File ▸ Save, Ctrl+S). Returns the path; the file is written in the same frame. |
| `(load-scene path)` | `scene.load` | edit | Replaces the scene with the file's, one undo step (File ▸ Open, Ctrl+O; `""`: a dialog). A file that does not read or parse is an error and nothing is queued; so is a file in which two objects share an id, or a number is NaN or infinite (brief 6-1). |
| `(signal-history name)` | `signal.history` | query | The signal's recorded `[t v]` samples, oldest first (the last 600). Empty for a name the bus does not hold. |
| `(signal-export path names)` | `signal.export` | editor | Writes CSV to `path` (must end in `.csv`): a header `t,<names>`, then one row per sample of the first signal; another signal's cell is its sample at the same `t`, else empty. An unknown name is an error. |

Each is one call through the registry, so a remote client or `gradiance op`
runs the same experiment (`gradiance op sim.step --n 120`, then poll
`gradiance op sim.time`). Paths are relative to the app's working directory
and are not confined to it (as `ui.screenshot`). Inside one script run every
read sees the state before the run (rule 3), so `(sim-step 60)` and
`(sim-time)` in one run read the old time: poll from a later run.

### Headless rollout: `gradiance run` (brief R-2)

```sh
gradiance run scene.ron --script probe.scm --steps 600 --csv out.csv --signals h,t
# a sweep is a shell loop; with no --csv the CSV goes to stdout
for g in 1 2 4; do
  sed "s/GAIN/$g/" sweep.scm > /tmp/s.scm
  gradiance run arm.ron --script /tmp/s.scm --steps 300 > "arm-$g.csv"
done
```

Loads the scene (`scene.load`), runs each script once in order (the
ordinary script doorway, so every verb works), finishes any steps a script
asked for with `(sim-step k)`, then advances exactly `--steps N` fixed
physics steps (`sim.step`) with no window and no GPU, and writes the
recorded signals as CSV (`signals_csv`, the function `signal.export` uses).
Rows: the state before the first step, then one per step, so `N + 1` rows
at `t = 0, dt, …` (plus any a script's own steps recorded). The 600-sample
plot window does not apply: a run keeps every sample. `--signals` names the
columns (default: every signal on the bus but `t`, or `t` alone). Two runs
of the same inputs give the same CSV byte for byte (one compute thread, one
fixed tick per update). Exit codes: `0` done, `1` a scene that does not
read or parse, a script error, an unknown signal or a failed write, `2` a
usage error (`gradiance run --help`). `run` is a host, not a client: it
builds its own app (`gradiance::run::rollout`) and needs no `--remote` app.

### Parts and the clipboard (brief R-3)

A scene file is also a part: `scene.insert` adds its bodies, joints, nodes
and groups to the current scene instead of replacing it, so a folder of
`.ron` files is a parts library with no new format. Copy and paste use the
same text: `edit.copy` writes the named bodies as a scene file (the joints
whose ends are all among them, a world pin with its body, the nodes on
them, their groups' names; every other setting at its default), and
`edit.paste` adds such a text. The text can go through chat or an MCP
client and come back as a paste.

| Verb | Id | Seam | What it does |
|---|---|---|---|
| `(insert-scene path at)` | `scene.insert` | edit | Adds a `.ron` file's objects with fresh ids, one `insert` step (File ▸ Insert…; `""`: a dialog). `at`: `(list)` keeps the file's positions, `(list x y)` centres the bodies there. Returns the new bodies' ids. |
| `(copy-ron targets)` | `edit.copy` | editor | The bodies as scene RON text, put on the clipboard too (Ctrl+C, Edit ▸ Copy on the selection). Nothing named: `""`, the clipboard is left alone. |
| `(paste-ron text at)` | `edit.paste` | edit | Adds scene RON text, as `scene.insert` adds a file (Ctrl+V: centred at the cursor; Edit ▸ Paste: where it was copied). |

Every inserted or pasted body, joint and node gets a new id from the
operation's context; the groups get new ids too, so the nesting is kept and
nothing joins a group already in the scene, and the names go with them.
The place is the centre of the box around the body positions. Signal
bindings and the other settings in the text are not applied (a paste of
settings is out of scope). Ctrl+C and Ctrl+V on a number field under the
pointer stay the field's (copy and paste its value), and a focused text
field takes every key. The clipboard is egui's; where no system clipboard
can be reached, a paste reads the app's last copy.

### Packing (brief R-4)

The layout optimizer (`docs/optimize-decision.md`) is reachable as
operations. `pack.run` starts a run on the listed bodies and returns at
once; the run iterates over the next frames, draws its ghost, and commits
the new poses as **one** `commit-transform` step when it finishes, exactly
as the Optimizer panel's runs do. The panel and the context menu invoke the
same `pack.run` with the selection, so a script run and a panel run over
the same bodies and the same rulebook commit the same layout: a run packs
its bodies in id order, whatever order they were listed or clicked in.

```scheme
(pack-set "clearance" 0.05)  ; the Optimizer window's rulebook (workstation config)
(pack (list 0 1 2))          ; arrange bodies 0, 1 and 2; one undo step when done
;; a later run:
(pack-status)                ; {"state":"applied","shrinkage":0.6,…} as JSON text
```

| Verb | Id | Seam | What it does |
|---|---|---|---|
| `(pack targets)` | `pack.run` | edit | Starts the solver on the listed bodies (fewer than two: a no-op; a target that names nothing: an error). One `commit-transform` step when the run finishes, or at `pack-apply` when `auto_apply` is off. Ground planes are left out. |
| `(pack-status)` | `pack.status` | query | `state` (`idle`, `running`, `finished` = held for `pack-apply`, `applied`, `cancelled`) and, past idle, the report: solver, status, bodies, iterations, progress, shrinkage, the start and best scores. The last run's report stays until the next start. |
| `(pack-apply)` | `pack.apply` | editor | Commits the best layout so far and ends the run (the panel's Apply). |
| `(pack-cancel)` | `pack.cancel` | editor | Ends the run and moves nothing (the panel's Cancel, Escape). |
| `(pack-get path)` / `(pack-set path v)` | `pack.get` / `pack.set` | query / config | A number or switch (0/1) of `PackConfig` by reflect-path: `clearance`, `max_iterations`, `iterations_per_frame`, `auto_apply`, `keep_groups`, `weights.extent`, … Workstation config: never saved, never undone. The solver, objective and the other choice fields are set in the Optimizer window. |

Inside one script run, `(pack …)` then `(pack-status)` reads the old state
(rule 3); poll `pack.status` from a later call, as for `sim-time`.

### The window: inspect and screenshot (brief U-3)

```scheme
(ui-inspect "tools:")        ; the tool buttons drawn last frame
(ui-screenshot "shot.png")   ; a PNG of the window, once the view settles
```

| Verb | Id | Seam | What it does |
|---|---|---|---|
| `(ui-inspect prefix)` | `ui.inspect` | query | The widgets the last frame drew whose name starts with `prefix` (`""`: all). Each row: `name`, `area`, `rect` `[x0 y0 x1 y1]`, `at` `[x y]` (the point to click), in egui logical points from the window's top left. Remote gets a JSON array of objects; Lisp a list of the same objects as JSON text. |
| `(ui-screenshot path)` | `ui.screenshot` | editor | Saves a PNG of the whole window (scene and panels) at `path` (must end in `.png`; relative to the working directory). Returns the path at once; the file follows once the view settles: at least 150 ms and 3 frames, a camera glide gets 500 ms more, a busy renderer (start-up, pipelines compiling) holds it up to 20 s. No undo step. |

Widget names come from the **widget registry** (`ui::widget_registry`, a
port of SolidCraft's `ui-registry`): an area publishes its rect under its own
name (`menu`, `transport`, `tools`, `right_dock`, `bottom_dock`,
`view_cube`, `status`, `search`, `pie`); a widget inside it is
`<area>:<name>` (`tools:Box`, one per palette tool, script tools included;
`search:field` and `search:<row>` while F3 search is open, `pie:<slice>`
while the tools pie is). A panel names a widget by calling
`widget_registry::publish_target` (rect and handle) or `publish_rect` while
it draws. Headless there is no egui pass: `ui-inspect` answers an empty list
and `ui-screenshot` logs that there is no window.

### The window: input by widget name (brief U-4)

```scheme
(ui-click (list 32 141))                   ; a window point: the palette's Select
(ui-drag (list 600 370) (list 700 370) 10) ; 10 frames of movement
(ui-key "Escape")                          ; clear the selection
(ui-key "E" 60)                            ; hold E for 60 frames (a key input)
(ui-text "1.5\n")                          ; type into the focused field
(ui-wheel (list 640 370) 1)                ; zoom in a notch
```

| Verb | Id | Seam | What it does |
|---|---|---|---|
| `(ui-click at)` | `ui.click` | editor | Left click at `at`: a widget name (`ui-inspect` lists them; `(ui-click "tools:Box")` picks the Box tool) or a window point `(list x y)`. Plays move, press, release, one frame each. |
| `(ui-click-button at button)` | `ui.click_button` | editor | The same click with a named button: `"left"`, `"middle"` or `"right"`. A middle click on a number field resets it to its default (brief U-8b). |
| `(ui-drag from to steps)` | `ui.drag` | editor | Left drag: move to `from`, press, move to `to` in `steps` frames (1–200), release. A tool's gesture commits its one undo step on the release, as for a hand-made drag. |
| `(ui-key chord [hold])` | `ui.key` | editor | Press and release a chord written as menus show it: `"Ctrl+Z"`, `"Ctrl+Shift+G"`, `"Delete"`, `"Escape"`, `"F1"`, `"A"`. Modifiers go down a frame before the key and up a frame after. `hold` (1–600 frames, default 1) keeps the key down that long: a held key drives a node-graph key input while the sim plays (brief N-2). |
| `(ui-text text)` | `ui.text` | editor | Types `text` into whatever has keyboard focus (click the field first), in one frame; a newline presses Enter. Typed text trips no key shortcut. |
| `(ui-wheel at lines)` | `ui.wheel` | editor | Move to `at`, then turn the wheel by `lines` (> 0: up, away from you). |

None records an undo step itself; the input it gives may (a drag that moves
a body is one step, as by hand). Each returns at once and its gesture plays
over the next frames, after any gesture already queued: one pointer event
per frame (Craft's injection rule), at most 2000 frames waiting. A widget
name resolves to the point the **last drawn frame** published; an unknown
name or a non-finite point is an error and nothing is queued.

The input is written inside the app as Bevy input messages, the ones winit
writes (`CursorMoved`, `MouseButtonInput`, `KeyboardInput`, `MouseWheel` and
the `WindowEvent` around each), and a move sets the window's cursor, so
egui and the tools both see real input; the OS pointer follows. Keys arrive
without window focus. Headless, keys and buttons still reach `ButtonInput`
(there is no cursor and no egui).

## Units

Every verb takes and returns SI: metres, radians, kilograms, seconds. A
number written in another unit goes through `q` first, which converts it
with the same parser a typed number field uses (`25 cm`, `90 deg`, `km/h`),
so a field and a script read units the same way. Short names are one
`define` each; the verbs do not add them, because `min`, `t`, `in` and `g`
are already names a script uses.

A ramp, a ball and a crate, in the units a drawing would give:

```scheme
;; units: mixed
(define (cm x) (q x "cm"))
(define (deg x) (q x "deg"))
(spawn-ground 0 (cm -250) 0)
(spawn-box-rot (cm -150) (cm -150) (cm 300) (cm 20) (deg 15))
(spawn-circle (cm -250) (cm 50) (q 120 "mm"))
(spawn-box (q 3 "ft") (cm -200) (cm 40) (cm 40))
```

The same scene in SI (a test checks that the two spawn the same bodies):

```scheme
;; units: SI
(spawn-ground 0 -2.5 0)
(spawn-box-rot -1.5 -1.5 3 0.2 0.2617993878)
(spawn-circle -2.5 0.5 0.12)
(spawn-box 0.9144 -2 0.4 0.4)
```

A converted value is a plain number again, so `(spawn-circle 0 0 (q 90
"deg"))` is not caught. To check what a value is meant to be, convert it
*to a quantity*: `(convert 90 "km/h" "speed")` is 25, while
`(convert 90 "deg" "height")` stops the run with `expected m, got rad`.
`(unit-of u)` reports what a unit measures in the symbols `quantity-unit`
uses, so `(equal? (unit-of "km/h") (quantity-unit "speed"))` is `#t`.

Units stop at the authoring boundary: the signal kernel and the node
graph's block constants stay plain numbers (`docs/units-decision.md` § P6).

## Examples

A floor with a stack of boxes:

```scheme
(spawn-ground 0 -2.5 0)
(let loop ((i 0))
  (when (< i 5)
    (spawn-box 0 (- (* i 0.3) 2.3) 0.25 0.25)
    (loop (+ i 1))))
```

Extend the right-click menu from an init script (`--script init.scm`):

```scheme
(register-action "Drop a ball"      "(spawn-circle (action-x) (action-y) 0.12)")
(register-action "Heavy gravity"    "(sim-set \"gravity.y\" -25)")
(register-action "Fill a shelf"
  "(let loop ((i 0)) (when (< i 8) (spawn-box (* i 0.3) 0 0.24 0.24) (loop (+ i 1))))")
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
    (spawn-circle (body-x i) (- (body-y i) 0.4) 0.05)
    (loop (+ i 1))))
```

Groups as editing layers: group a filter result, then edit the group as one
thing. Every read in a run sees the pre-run scene, so the new group is edited
from a second run (here a context-menu action):

```scheme
(group (bodies-where (list "kind:box")))
(register-action "Style the row"
  "(begin (name-group 0 \"row\") (group-distribute 0 \"hue\" 0 240 \"x\")
          (group-set 0 \"friction\" 0.2) (group-distribute 0 \"density\" 0.5 5 \"x\"))")
```

A group, once named and styled, can be split again: `(decompose 0 "color")`
leaves one group per fill colour, `(decompose 0 "islands")` one per set of
jointed bodies, and `(decompose 0 "dissolve")` removes the group. Like Ctrl+G,
each of these is one undo step; nested groups around and inside it are kept.

## Adding a verb (contributor recipe)

### Declare an operation (every verb; briefs U-1, U-2a, U-2b)

An operation is declared **once**, in the layer that owns it, and every
doorway is generated from that declaration: the Lisp verb, the remote
`gradiance.op {"id", "params"}` call, a menu row's `InvokeOp`, the catalog
row the console and `gradiance.ops` show, and the key link table's rows.
Every verb works this way (since brief U-2b) except the few **VM
built-ins** below. The operations live with their owners: scene edits and
queries in `crates/gradiance-command/src/ops/` (split by family), the
camera-feel pair in `crates/gradiance-interaction/src/ops.rs`, the editor
and signal verbs in `crates/gradiance-script/src/ops.rs`. The full list is
`docs/ops.md`.

1. In the owning layer, write a parameter struct that derives
   `serde::Deserialize` (with `#[serde(deny_unknown_fields)]`) and
   `bevy::reflect::Reflect`. The **field order is the Lisp argument order**.
   Use `gradiance-units` quantities (`Length`, `Angle`, …) for physical
   values; they decode from plain JSON numbers. Name a scene object with
   `gradiance_op::Target` (an index, an id string, `"selection"`, or
   `{"at": [x, y]}`); a set is `Vec<Target>` (`(list i …)` in Lisp).
   `"selection"` resolves only where the interaction layer publishes its
   selection mirror (menus, keys, the remote `id` form). A script run has
   no selection mirror, so there it is a visible error (scripting rule 2).
2. `impl gradiance_op::Operation`: `META` (id `layer.verb`, Lisp name, doc,
   seam, menu path, chord, the undo label the command records, and an
   example params object) and `lower(self, &OpContext)`. `lower` reads old
   values from a context mirror (`ctx.get::<SceneSnapshot>()`), takes new ids
   from `ctx.new_id()`, and returns an `OpOutput`: `.message(intent)`,
   `.setting(&mirror, path, value)?`, `.returning(value)`. It never touches
   the world. A query returns NaN for "nothing there"; JSON carries it as
   `null` and the Lisp doorway turns a query's `null` back into NaN. An
   editor operation that fills a table writes a message its layer drains
   (`gradiance-script`'s `EditorRequest`); the seam and the undo label say
   whether it is an undo step (`camera.set`: workstation config, none;
   `sim.set`: scene content, one `settings` step).
3. Register it in the layer's plugin: `registry.register::<MyOp>()`. If it
   reads a new mirror, publish it from that layer's context filler. A mirror
   too costly to build on every call (`scene.dump`'s whole-scene record) is
   **gated**: `OpContextFillers::add_for(&["layer.verb"], filler)` runs it
   only for a call of that operation or a script run whose source names its
   verb.
4. A menu row or a key writes `InvokeOp::with("layer.verb", json!({…}))`
   (or `InvokeOp::new` without params), never the intent. A key binding is
   one row of `KEY_OPS` in `crates/gradiance-interaction/src/input.rs`; a
   menu row that repeats a key writes `key_invocations(action)`.
5. Regenerate the reference: `UPDATE_OPS_DOC=1 cargo test --test it
   docs_ops_md_is_current` rewrites `docs/ops.md`.
6. Nothing else. The conformance tests (`tests/it/op_conformance.rs`) iterate
   the registry and run the new operation through every doorway from its
   `example` against a fixed fixture: equal outputs, schema round trip, the
   declared undo step, `docs/ops.md` current. They also check every key row
   against a fixture selection, and that `gradiance-ui` and
   `gradiance-interaction` declare no intent writer outside an allow list
   of gesture tools and inspector fields (each row with its reason).
   The undo matrix (`tests/it/undo_matrix.rs`, brief 6-1) runs every
   operation that records or moves a history step from its example too:
   dump, call, dump, undo, dump, redo, dump. Undo must give the first dump
   back exactly and redo the second. An example that changes nothing on the
   fixture fails it unless `IDENTITY_ON_FIXTURE` lists it with a reason.

### VM built-ins (not operations)

A handful of verbs exist only inside a VM run: the invocation-context reads
(`action-x`, `action-y`, `tool-count`, `tool-x`, `tool-y`: there is no
action or tool outside a run) and the introspection pair (`ops`,
`describe`: the remote surface has `gradiance.ops`). Seven in all; the
panel verbs became `panel.*` operations in brief U-3b. The remote surface
refuses a built-in's name (it exists only inside a script run). They are the `BUILTINS` table in
`crates/gradiance-script/src/registry.rs` (each row says why it is not an
operation) and a steel builtin in `bridge.rs` under the same `name::`
constant. Do not add one for anything a remote caller or a menu could use:
declare an operation.

### Joints: relationships, not just objects

A multibody sandbox is mostly about *how things are connected*, and until these
verbs a script could author bodies but not a single constraint. All three of the
engine's joint kinds are reachable:

```scheme
;; a three-link chain, hinged at the shared edges
(begin (spawn-box 0 0 0.3 0.08) (spawn-box 0.3 0 0.3 0.08) (spawn-box 0.6 0 0.3 0.08))
(begin (hinge 0 1 0.15 0) (hinge 1 2 0.45 0))
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

`(spring …)` is authored like the spring tool's strut, mass-relative: a
natural frequency in Hz (the tool's default 1.5) and a damping ratio (0 =
undamped, 1 = critical; default 0.1). The physics seam derives the stiffness
from the connected bodies' current mass, so `(spring a b 1.5 0.1)` bounces
with the same period on a pebble and on a truck, and keeps it when a body's
density changes (brief 4b-4; ranges in `gradiance_domain::tuning`).

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
and neither is a resize. Mirroring is `(mirror-h targets)` / `(mirror-v
targets)` (Edit ▸ Mirror horizontally / vertically): world axes through the
middle of the bodies' bounds; each body turns to its reflected angle and its
shape reflects across one of its own axes, so a box stays a box and a CSG body
stays a tree, and mirroring twice gives the scene back. Joint anchors, slider
axes and node offsets go through the same body map (on any scale); a hinge
whose bodies all mirror gets its limits and motor turned over.

`(set-friction …)` moves both the static and dynamic coefficients together,
which is what the inspector does: authoring two numbers that are almost always
equal is friction the UI deliberately does not expose.

### Why `(delete i)` and not `(delete-selection)`

The Edit menu's delete acts on the selection. A verb cannot: `Selection` lives
in `gradiance-interaction`, which sits **above** `gradiance-script` in the layer
graph, so the script layer cannot read it — the same asymmetry the panel
operations resolve with a mirror (their owner, `gradiance-ui`, publishes it).

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

The panel verbs are `gradiance-op` operations declared by `gradiance-ui`
(`crates/gradiance-ui/src/panel_ops.rs`, brief U-3b): `panel.show`,
`panel.hide`, `panel.toggle` (editor seam, no undo step), `panel.open` and
`panel.list` (queries; Lisp `panel-open?` and `panels`). Lisp, the remote
surface and the catalog get them from those declarations. They resolve
against **one** table — `Panels::named` in `crates/gradiance-ui/src/panels.rs`
— which is the same table the View menu renders. Adding a panel is one row
there and it appears in both, so the menu and the API cannot drift; a unit
test asserts every registry name has a menu label.

The operations add no mutation path. A write lowers to a `PanelRequest`
message; the UI applies it through `PanelToggle::set_open`, which is exactly
what a menu click does. Every call reads a `PanelMirror` the UI layer puts
into the operation context, so an unknown name is refused at the call with
the list of known names, and the reads see the state as the run began (one
snapshot per run). The panel resources exist headless too, so tests and a
headless remote client see the same panels, undrawn.

Workspaces and areas (`crates/gradiance-ui/src/workspaces.rs`, brief U-10)
follow the same shape: `layout.save`, `layout.apply`,
`layout.workspace.select`, `layout.workspace.delete` and `panel.swap` lower
to messages the UI applies (visibility through `PanelToggle`, the dock trees
through `dock_sync`); `layout.workspace.list` reads a `WorkspaceMirror`. The
docks' trees are kept in `Update` by `workspaces::sync_docks`, so a headless
run has them too and a test can read a dock's tile order.

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
| `gradiance.ops` | none | every op: `name`, `signature`, `doc`, `category`, `args`; a registry operation also has `id`, `fields`, `schema` (a JSON Schema of its params, from reflection) and `example` |
| `gradiance.op` | `{"id": "<op id>", "params": {...}}`, or `{"name": "<lisp name>", "args": [...]}` (`args` optional) | the op's value as JSON (a query's NaN is `null`) |
| `gradiance.query` | same | same, but only `query` ops are accepted |

```sh
curl -s localhost:15702 -d '{"jsonrpc":"2.0","id":1,"method":"gradiance.op",
  "params":{"name":"spawn-box","args":[0,1,1.6,1.2]}}'
# {"jsonrpc":"2.0","id":1,"result":"6f1c…"}   (the new body's handle)
```

**The `id` form is one operation.** `{"id": "body.place", "params": {"i":
0, "x": 1.5, "y": 0, "angle": 0}}` is decoded by the registry's serde decoder
(unknown or missing fields are errors), lowered against a fresh snapshot and
applied through the seams it names. No Scheme text is built and no console
line is written. An edit becomes a command in the next frame's `Update`.

**The `name` form is the same operation, positionally.** `{"name":
"spawn-box", "args": [0, 1, 1.6, 1.2]}` maps the args to the named params in
declared order (the mapping the Lisp doorway uses; trailing optional params,
shown as `[hold]` in a signature, may be left off) and goes through the
registry exactly like the `id` form: no Scheme text, no console line. The
surface is **registry-only**: a **VM built-in** (`ops`, `describe`, the
invocation-context reads) exists only inside a script run, so naming one is
an invalid-params error that says so (`gradiance.ops` is the remote `(ops)`).
Two calls are two invocations (two snapshots); to compose edits atomically,
`register-action` a helper and invoke it from a script.

**Errors** are JSON-RPC errors:

| Code | When | `message` |
|---|---|---|
| `-32602` (invalid params) | unknown op, a VM built-in's name, bad params shape, a param that does not decode, a refused value, a non-query op sent to `gradiance.query` | the op's id (or Lisp name) once, then what was wrong |

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

### Command line and MCP

Two client doorways ship in the same binary (brief U-6). Both talk to a
running `gradiance --remote` and read the catalog from it, so they list and
accept exactly what the app's registry holds. `--port N` (or
`GRADIANCE_PORT`) selects the port; the default is 15702.

```sh
gradiance ops body.                          # id, category, signature, doc
gradiance ops --json                         # the rows, with schema and example
gradiance op body.spawn_box --x 0 --y 1 --w 1.6 --h 1.2
gradiance op spawn-box 0 1 1.6 1.2           # positional, declared field order
gradiance op ui.click --target tools:Box
gradiance op body.place --params '{"i": 0, "x": 1, "y": 0, "angle": 0}'
```

`op` builds a **named** params object and calls the `id` form, so the app's
one serde decoder decides what is accepted. A flag's text follows its
field's schema type: a `string` field keeps the text; any other field is
read as JSON, then as a comma list (`1,2` is `[1, 2]`), then as text (an
object id, `selection`). The value prints as JSON. Exit codes: 0 done, 1 the
app refused the operation, 2 a bad command line, 3 no app answers.

`gradiance mcp` serves the **Model Context Protocol** on stdio
(newline-delimited JSON-RPC; `initialize`, `ping`, `tools/list`,
`tools/call`). Four tools, not one per operation:

| Tool | Arguments | Does |
|---|---|---|
| `ops` | `prefix` (optional) | the catalog rows: id, name, doc, category, fields, schema, example |
| `query` | `op`, `params` or `args` | one `query` operation (read-only hint set) |
| `execute` | `op`, `params` or `args` | any one operation |
| `batch` | `calls`: `[{op, params or args}]` | several in order; stops at the first error |

`op` is an id (with named `params`) or a Lisp name (with positional
`args`). A refused operation is a tool result with `isError: true`. A batch
is not one undo step: each call is its own, as successive remote calls are.
`gradiance mcp --launch` starts `gradiance --remote <port>` when nothing
answers and stops it when the client closes the session. A client
configuration:

```json
{"mcpServers": {"gradiance": {"command": "gradiance", "args": ["mcp", "--launch"]}}}
```

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
