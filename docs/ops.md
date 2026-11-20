# Operations

<!-- Generated from the operation registry by `tests/it/op_conformance.rs` (`docs_ops_md_is_current`). Do not edit by hand: run `UPDATE_OPS_DOC=1 cargo test --test it docs_ops_md_is_current`. -->

Every Lisp verb, remote call, menu row and key edit is one of these 140 operations, declared once in the layer that owns it (`gradiance-op`; the recipe is in `docs/scripting.md`), or one of the 7 VM built-ins at the end. Remote: `gradiance.op {"id", "params"}` takes the id and the named params; `{"name", "args"}` takes the Lisp name and the params in call order.

Seams: **edit** lowers to an undoable intent; **config** writes a settings resource; **query** returns a value and writes nothing; **editor** writes non-authored editor state. A query's NaN ("nothing there") is `null` over remote.

## `action`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `action.register` | `(register-action label source)` | editor | — | — | Surface a named action (label + lisp source) in the editor's menus. | `{"label": "Fill", "source": "(spawn-box 0 0 1 1)"}` |

## `body`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `body.array` | `(array-copies targets count step rows cross)` | edit | `array` | — | Copy the listed bodies count times at step (x y, metres), plus rows more rows at cross: one undo step, as the Ctrl-drag array gesture. A count of 0 is a no-op; at most 512 per axis. | `{"targets": [0], "count": 2, "step": [1.2, 0], "rows": 0, "cross": [0, 0]}` |
| `body.chain` | `(chain path link width a b pins)` | edit | `spawn-chain` | — | Lay a chain of capsule links (about link long, width wide) hinged end to end along path [x0 y0 x1 y1 …]; pins "free", "bodies" (start to body a, end to body b, when they name one) or "world" (else to the world). Returns the links' ids. | `{"path": [-1, 1, 0, 0, 1, 1], "link": 0.25, "width": 0.06, "a": -1, "b": -1, "pins": "world"}` |
| `body.count` | `(body-count)` | query | — | — | Number of authored bodies in the committed scene. | `{}` |
| `body.count_at` | `(count-at x y)` | query | — | — | Number of bodies whose shape contains the world point (x, y). | `{"x": 0, "y": 1}` |
| `body.cut` | `(cut ax ay bx by width)` | edit | `cut` | — | Sever every body crossed by the stroke a→b of the given width (metres). | `{"ax": -2, "ay": 1, "bx": 2, "by": 1, "width": 0.05}` |
| `body.delete` | `(delete i)` | edit | `delete` | Edit/Delete selection, Delete | Delete the i-th body (id order, same index as body-x) — undoable. | `{"i": 0}` |
| `body.density` | `(body-density i)` | query | — | — | Mass density of the i-th body. | `{"i": 0}` |
| `body.fillet_merge` | `(fillet-merge a b radius)` | edit | `fillet-merge` | Context/Merge with fillet | Fuse bodies a and b into one with a smooth fillet of radius metres where they meet (CSG smooth union); a survives. Bridges a gap under radius/2. Errors unless radius > 0. | `{"a": 0, "b": 1, "radius": 0.25}` |
| `body.friction` | `(body-friction i)` | query | — | — | Coulomb friction of the i-th body. | `{"i": 0}` |
| `body.index_at` | `(body-index-at x y)` | query | — | — | Index of a body whose shape contains (x, y) (first in id order); -1 if none. | `{"x": 0, "y": 1}` |
| `body.intersect` | `(intersect a b)` | edit | `intersect` | Context/Intersect | Keep only where bodies a and b overlap (CSG intersect) on a; b is consumed. | `{"a": 0, "b": 1}` |
| `body.is_static` | `(body-static? i)` | query | — | — | Whether the i-th body is static (1) or not (0). | `{"i": 0}` |
| `body.merge` | `(merge a b)` | edit | `merge` | Context/Merge into one body | Fuse bodies a and b into one CSG union; a survives. | `{"a": 0, "b": 1}` |
| `body.nearest_at` | `(nearest-at x y)` | query | — | — | Index of the body whose centre is nearest (x, y); -1 if none. | `{"x": 0, "y": 1}` |
| `body.nearest_dist` | `(nearest-dist x y)` | query | — | — | Distance from (x, y) to the nearest body centre; -1 if the scene is empty. | `{"x": 0, "y": 1}` |
| `body.place` | `(place i x y angle)` | edit | `commit-transform` | — | Move the i-th body to (x, y) and rotate it to `angle` radians — one undo step. | `{"i": 0, "x": 1.5, "y": 0.25, "angle": 0.5}` |
| `body.restitution` | `(body-restitution i)` | query | — | — | Bounciness of the i-th body. | `{"i": 0}` |
| `body.rot` | `(body-rot i)` | query | — | — | Rotation (radians) of the i-th body; NaN if out of range. | `{"i": 0}` |
| `body.scale` | `(scale i fx fy)` | edit | `scale` | — | Resize the i-th body by (fx, fy) about its own centre, along its own axes. | `{"i": 0, "fx": 2, "fy": 1.5}` |
| `body.set_color` | `(set-color i r g b)` | edit | `property-edit` | — | Set the i-th body's fill to (r, g, b), each in 0..1 — undoable. | `{"i": 0, "r": 0.9, "g": 0.3, "b": 0.1}` |
| `body.set_density` | `(set-density i v)` | edit | `property-edit` | — | Set the i-th body's mass density (area x density = mass) — undoable. | `{"i": 0, "v": 3}` |
| `body.set_friction` | `(set-friction i v)` | edit | `property-edit` | — | Set the i-th body's Coulomb friction (static and dynamic) — undoable. | `{"i": 0, "v": 0.9}` |
| `body.set_restitution` | `(set-restitution i v)` | edit | `property-edit` | — | Set the i-th body's bounciness, 0 = dead, 1 = perfectly elastic — undoable. | `{"i": 0, "v": 0.8}` |
| `body.set_static` | `(set-static i on)` | edit | `property-edit` | — | Make the i-th body static when `on` is non-zero, dynamic otherwise — undoable. | `{"i": 0, "on": 1}` |
| `body.sketch` | `(sketch path width)` | edit | `spawn-body` | — | Author one body from a freehand stroke width wide along path [x0 y0 x1 y1 …]: simplified into capsule segments joined as one union, or filled as a polygon when the stroke closes on itself. Returns its handle. | `{"path": [-1, 1, -0.5, 1.4, 0, 1, 0.5, 1.4, 1, 1], "width": 0.1}` |
| `body.spawn_box` | `(spawn-box x y w h)` | edit | `spawn-body` | — | Author a box body of size w×h centred at (x, y), in metres; returns its handle. | `{"x": 0.5, "y": 2, "w": 1.6, "h": 1.2}` |
| `body.spawn_box_rot` | `(spawn-box-rot x y w h angle)` | edit | `spawn-body` | — | Author a w×h box centred at (x, y), in metres, rotated by angle radians; returns its handle. | `{"x": 2, "y": 3, "w": 1, "h": 0.5, "angle": 0.3}` |
| `body.spawn_circle` | `(spawn-circle x y r)` | edit | `spawn-body` | — | Author a circle body of radius r centred at (x, y), in metres; returns its handle. | `{"x": 0, "y": 3, "r": 0.5}` |
| `body.spawn_ground` | `(spawn-ground x y angle)` | edit | `spawn-body` | — | Author a fixed ground half-plane through (x, y) (metres), tilted by angle radians. | `{"x": 0, "y": -2, "angle": 0.1}` |
| `body.subtract` | `(subtract a b)` | edit | `subtract` | Context/Subtract from first selected | Cut body b's shape out of body a (CSG subtract); b is consumed, a split result becomes pieces. | `{"a": 0, "b": 1}` |
| `body.touch_count` | `(touch-count i)` | query | — | — | How many bodies the i-th body is currently touching; NaN if out of range. | `{"i": 0}` |
| `body.where` | `(bodies-where clauses)` | query | — | — | Indices of the bodies matching every clause (kind:circle, group:0, ungrouped, palette:3, friction:0.2..0.5, not:…). | `{"clauses": ["kind:circle", "grouped"]}` |
| `body.x` | `(body-x i)` | query | — | — | X of the i-th body (bodies ordered by id); NaN if out of range. | `{"i": 0}` |
| `body.xor` | `(xor a b)` | edit | `xor` | Context/Xor | Keep what lies in exactly one of bodies a and b (CSG xor) on a; b is consumed, a split result becomes pieces. | `{"a": 0, "b": 1}` |
| `body.y` | `(body-y i)` | query | — | — | Y of the i-th body; NaN if out of range. | `{"i": 0}` |

## `camera`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `camera.get` | `(camera-get path)` | query | — | — | Read a camera-feel setting: "zoom_step", "pan_speed", "orbit_speed_deg"; NaN if the path does not resolve. | `{"path": "zoom_step"}` |
| `camera.set` | `(camera-set path value)` | config | — | — | Set a camera-feel setting by reflect-path (workstation config, never undone). | `{"path": "pan_speed", "value": 900}` |

## `chrome`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `chrome.apply` | `(chrome-apply name)` | config | — | — | Draw the editor panels with a chrome preset (blender, blender-blue, stock); workstation config, never undone. | `{"name": "blender"}` |
| `chrome.list` | `(chromes)` | query | — | — | Every chrome preset's name, in menu order (the first is the default). | `{}` |

## `edit`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `edit.adjust_last` | `(adjust-last field value)` | edit | — | — | Set one param of the last operation (field "w", "step.0") and run it again in place of its undo step: still one step. Errors when the newest step is not an operation's. | `{"field": "r", "value": 0.75}` |
| `edit.copy` | `(copy-ron targets)` | editor | — | Edit/Copy, Ctrl+C | Copy bodies, the joints between them, their nodes and their groups to the clipboard as scene RON text (what a save writes, cut to them); returns the text ("" when nothing is named, and the clipboard is left alone). No undo step. | `{"targets": [3, 4]}` |
| `edit.last_op` | `(last-op)` | query | — | — | The operation that made the newest undo step, as {id, params} (what Adjust Last Operation edits); null when there is none. | `{}` |
| `edit.paste` | `(paste-ron text at)` | edit | `insert` | Edit/Paste, Ctrl+V | Add scene RON text (from edit.copy or a .ron file) to the scene with fresh ids, one undo step. `at`: [] keeps its positions, [x y] centres the bodies there (Ctrl+V: the cursor). Returns the new bodies' ids. | `{"text": "(version: 8, app_version: \"\", bodies: [(id: (\"00000000-0000-0000-0000-00000000000a\"), pose: (pos: (0.0, 1.0), rot: 0.0), shape: Box(width: 1.0, height: 1.0))], joints: [], environment: ())", "at": [2, 3]}` |
| `edit.redo` | `(redo)` | edit | — | Edit/Redo, Ctrl+Shift+Z | Redo the last undone command — the same step Edit ▸ Redo takes. | `{}` |
| `edit.undo` | `(undo)` | edit | — | Edit/Undo, Ctrl+Z | Undo the last command — the same step Edit ▸ Undo takes. | `{}` |

## `group`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `group.create` | `(group targets)` | edit | `group` | Edit/Group, Ctrl+G | Group the listed bodies (id order) like Ctrl+G and return the new group's index; fewer than two distinct bodies is a no-op. | `{"targets": [0, 1]}` |
| `group.decompose` | `(decompose group key)` | edit | `regroup` | — | Split the group into one group per colour, kind or joint island, or dissolve it — one undo step. | `{"group": 0, "key": "dissolve"}` |
| `group.distribute` | `(group-distribute group prop lo hi by)` | edit | `property-edit` | — | Spread a body property over the members from lo to hi, ordered by `by` (order, x, y, size, value) — one undo step. | `{"group": 0, "prop": "density", "lo": 0.5, "hi": 5, "by": "x"}` |
| `group.list` | `(groups)` | query | — | — | Every group's name in group order ("" when unnamed); a group index counts into this list. | `{}` |
| `group.members` | `(group-members group)` | query | — | — | The body indices of the group's members (any nesting depth), as a list. | `{"group": 0}` |
| `group.name` | `(group-name group)` | query | — | — | The group's name ("" when unnamed; NaN when no such group). | `{"group": 0}` |
| `group.rename` | `(name-group group name)` | edit | `name-group` | — | Name the group ("" removes the name) — undoable, saved with the scene. | `{"group": 0, "name": "wheels"}` |
| `group.scatter` | `(group-scatter group prop lo hi seed)` | edit | `property-edit` | — | Set a body property on every member to a seeded random value in lo..hi — one undo step. | `{"group": 0, "prop": "hue", "lo": 0, "hi": 300, "seed": 7}` |
| `group.set` | `(group-set group prop v)` | edit | `property-edit` | — | Set one body property (friction, density, hue, palette, depth, …) on every member — one undo step. | `{"group": 0, "prop": "friction", "v": 0.2}` |
| `group.set_color` | `(group-set-color group r g b)` | edit | `property-edit` | — | Set every member's fill to (r, g, b), each in 0..1 — one undo step. | `{"group": 0, "r": 0.9, "g": 0.3, "b": 0.1}` |
| `group.ungroup` | `(ungroup targets)` | edit | `ungroup` | Edit/Ungroup, Ctrl+Shift+G | Peel the listed bodies' outermost group (id order) like Ctrl+Shift+G; inner groups survive. | `{"targets": [3]}` |

## `joint`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `joint.count` | `(joint-count)` | query | — | — | Number of authored joints in the committed scene. | `{}` |
| `joint.delete` | `(delete-joint i)` | edit | `delete-joint` | Context/Delete joint, Delete | Remove the i-th joint (id order, as joint-count) — undoable. | `{"i": 0}` |
| `joint.hinge` | `(hinge a b x y)` | edit | `spawn-joint` | — | Hinge bodies a and b at world point (x, y); b < 0 pins a to the world. | `{"a": 0, "b": 1, "x": -0.2, "y": 1}` |
| `joint.slider` | `(slider a b x y ax ay)` | edit | `spawn-joint` | — | Prismatic joint at (x, y) sliding along world axis (ax, ay); b < 0 pins to the world. | `{"a": 0, "b": 1, "x": -0.2, "y": 1, "ax": 1, "ay": 0}` |
| `joint.spring` | `(spring a b frequency damping_ratio)` | edit | `spawn-joint` | — | Spring-damper strut between the centres of a and b; rest length is their current distance. | `{"a": 0, "b": 2, "frequency": 1.5, "damping_ratio": 0.1}` |

## `layout`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `layout.apply` | `(layout-apply)` | editor | — | — | Put the docks back to the active workspace's saved layout. | `{}` |
| `layout.save` | `(layout-save name)` | editor | — | — | Save the docks' layout (editors, splits, sizes) as a workspace and make it active; a saved one of that name is replaced. Workstation config: saved with the scene, never undone. | `{"name": "Mine"}` |
| `layout.workspace.delete` | `(workspace-delete name)` | editor | — | — | Delete a saved workspace by name (presets stay). Deleting the active one makes Layout active and leaves the docks as they are. | `{"name": "Mine"}` |
| `layout.workspace.list` | `(workspaces)` | query | — | — | Every workspace in tab order: its name, whether it is a preset, and whether it is active. | `{}` |
| `layout.workspace.select` | `(workspace-select name)` | editor | — | — | Switch to a workspace by name (a preset: Layout, Scripting, Signals; or a saved one): its tab becomes active and the docks take its layout. | `{"name": "Scripting"}` |

## `lighting`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `lighting.set` | `(lighting-set path value)` | config | `settings` | — | Set a lighting setting by reflect-path, e.g. "ambient", "lights[0].elevation_deg", "ssao" (a settled edit is one undo step). | `{"path": "lights[0].elevation_deg", "value": 30}` |

## `look`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `look.apply` | `(look-apply name)` | config | `settings` | — | Apply a scene look by name (studio, blueprint, poster): palette tone, overlay ink, render, lighting and scenery presets — one undo step. | `{"name": "blueprint"}` |
| `look.list` | `(looks)` | query | — | — | Every scene look's name, in menu order (the first is the default). | `{}` |

## `node`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `node.thruster` | `(thruster body x y angle thrust)` | edit | `spawn-node` | — | Attach a thruster to body at world point (x, y), pushing with thrust newtons along the world angle (radians); returns its handle. | `{"body": 0, "x": 0, "y": 0, "angle": 1.5708, "thrust": 10}` |

## `pack`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `pack.apply` | `(pack-apply)` | editor | — | — | Apply the packing run's best layout now and end it (the Optimizer's Apply): that is pack's commit-transform step; nothing when no run is loaded. | `{}` |
| `pack.cancel` | `(pack-cancel)` | editor | — | — | End the packing run and leave every body where it was (the Optimizer's Cancel, Escape). No undo step. | `{}` |
| `pack.get` | `(pack-get path)` | query | — | — | Read a number of the packing rulebook by reflect-path ("clearance", "max_iterations", "weights.extent", …); NaN if the path does not resolve to a number. | `{"path": "clearance"}` |
| `pack.run` | `(pack targets)` | edit | `commit-transform` | — | Start the layout solver on the listed bodies (id order; the Optimizer's rulebook, pack-set) and commit their new poses as one commit-transform step when it finishes (at pack-apply when auto_apply is off). Returns at once; read pack-status. Fewer than two distinct bodies is a no-op. | `{"targets": [0, 1, 2]}` |
| `pack.set` | `(pack-set path value)` | config | — | — | Set a number or switch (0/1) of the packing rulebook by reflect-path, as the Optimizer window does (workstation config, never saved or undone). | `{"path": "clearance", "value": 0.05}` |
| `pack.status` | `(pack-status)` | query | — | — | The packing run as an object: state (idle, running, finished, applied, cancelled) and, past idle, the solver, status, bodies, iterations, progress, shrinkage and the start and best scores (extent, fill, overlap, violations, min_gap, bounds). | `{}` |

## `panel`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `panel.hide` | `(panel-hide name)` | editor | — | — | Close an editor panel by name. | `{"name": "properties"}` |
| `panel.list` | `(panels)` | query | — | — | Every editor panel in View-menu order: its name (what the panel operations take) and whether it is open. | `{}` |
| `panel.open` | `(panel-open? name)` | query | — | — | Whether an editor panel is shown, by name. | `{"name": "properties"}` |
| `panel.show` | `(panel-show name)` | editor | — | — | Open an editor panel by name, as its View-menu row does (see panel.list for the names). | `{"name": "properties"}` |
| `panel.swap` | `(panel-swap from to)` | editor | — | — | Switch the dock tile showing one editor to another, in place, as the tile's editor-type menu does. If `to` is shown elsewhere the two exchange tiles; if `from` is not shown, `to` just opens. Right dock: outliner, groups, depth, plot, properties, console; bottom: nodes. | `{"from": "outliner", "to": "console"}` |
| `panel.toggle` | `(panel-toggle name)` | editor | — | — | Flip an editor panel's visibility by name. | `{"name": "properties"}` |

## `quantity`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `quantity.between` | `(quantity-between name a b)` | query | — | — | A two-body catalog quantity (distance) between bodies a and b, in SI; NaN if either is out of range. | `{"name": "distance", "a": 0, "b": 1}` |
| `quantity.list` | `(quantities)` | query | — | — | Every quantity name in the catalog (what quantity, quantity-between and the sensor ports read). | `{}` |
| `quantity.read` | `(quantity name i)` | query | — | — | A catalog quantity of the i-th body, or of the i-th joint for a joint-* quantity, in SI; NaN if out of range or not simulating. | `{"name": "speed", "i": 0}` |
| `quantity.unit` | `(quantity-unit name)` | query | — | — | The SI unit symbol a quantity reads in ("" for a count). | `{"name": "speed"}` |

## `render`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `render.ramp` | `(render-ramp interp stops)` | config | `settings` | — | Replace the toon colour ramp: interp (constant, linear) and stops as flat position r g b groups; 0.5 grey shows a fill as authored (a settled edit is one undo step). | `{"interp": "constant", "stops": [0.0, 0.3, 0.25, 0.35, 0.6, 0.5, 0.5, 0.5]}` |
| `render.set` | `(render-set path value)` | config | `settings` | — | Set a render (toon) setting by reflect-path, e.g. "dither", "white_point", "ramp.stops[1].position", "ramp.stops[0].color.r" (a settled edit is one undo step). | `{"path": "dither", "value": 0.1}` |

## `scene`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `scene.dump` | `(scene-dump)` | query | — | — | The whole scene as JSON: the record a save writes (bodies with their groups and depth, joints with their anchors, nodes, and the environment). Lisp gets the JSON text. | `{}` |
| `scene.insert` | `(insert-scene path at)` | edit | `insert` | File/Insert… | Add a .ron file's bodies, joints, nodes and groups to the scene with fresh ids, one undo step ("" = a dialog). `at`: [] keeps the file's positions, [x y] centres the bodies there. Returns the new bodies' ids. | `{"path": "tests/fixtures/scenes/pendulum.ron", "at": [6, 2]}` |
| `scene.load` | `(load-scene path)` | edit | `load-scene` | File/Open…, Ctrl+O | Replace the scene with a .ron file's (relative to the working directory; "" = a dialog): one undo step, as File ▸ Open. An unreadable file is an error and changes nothing. | `{"path": "tests/fixtures/scenes/pendulum.ron"}` |
| `scene.new` | `(new-scene)` | edit | `load-scene` | File/New scene | Replace the scene with an empty one (default environment) — one undo step, as File ▸ New scene. | `{}` |
| `scene.save` | `(save-scene path)` | editor | — | File/Save, Ctrl+S | Write the scene to a .ron file (relative to the working directory; "" = the last path, else a dialog). Returns the path; the file is written this frame. No undo step. | `{"path": "target/scene.ron"}` |

## `scenery`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `scenery.set` | `(scenery-set path value)` | config | `settings` | — | Set a scenery setting by reflect-path, e.g. "perspective_deg", "back_offset", "back_visible" (a settled edit is one undo step). | `{"path": "perspective_deg", "value": 20}` |

## `signal`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `signal.computed` | `(defsignal name expr)` | config | `settings` | — | Declare a computed signal from an RPN expression over other signals (e.g. "t sin amp *"). | `{"name": "osc", "expr": "t sin 2 *"}` |
| `signal.export` | `(signal-export path names)` | editor | — | — | Write signals' recorded history to a .csv file: a header t,name1,name2,... then one row per sample of the first signal (a cell is empty where another signal has no sample at that t). Returns the path. No undo step. | `{"path": "target/signals.csv", "names": ["t"]}` |
| `signal.get` | `(signal-get name)` | query | — | — | Current value of a named signal on the bus; NaN if unset. | `{"name": "t"}` |
| `signal.history` | `(signal-history name)` | query | — | — | A signal's recorded samples, oldest first, as a list of [t v] (t in simulated seconds; the last 600, recorded while the sim plays or steps); empty if the bus has no such signal. | `{"name": "t"}` |
| `signal.key` | `(defkey name key [negative])` | config | `settings` | — | Declare a key input: a signal that is 1 while key is held in play (0 paused or while a field has the keyboard); with negative, -1 while that key is held and 0 when both are. While the sim plays the key is the input's, not the editor's. | `{"name": "throttle", "key": "E", "negative": "Q"}` |
| `signal.param` | `(defparam name value min max)` | config | `settings` | — | Declare a tunable signal parameter (an auto-slider) published on the bus. | `{"name": "amp", "value": 2, "min": 0, "max": 10}` |
| `signal.set` | `(signal-set name value)` | editor | — | — | Publish a named value on the signal bus (drives bindings; docs/signal-dataflow.md). | `{"name": "excitement", "value": 7}` |

## `sim`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `sim.get` | `(sim-get path)` | query | — | — | Read a simulation setting by reflect-path, e.g. "gravity.y", "speed"; NaN if the path does not resolve. | `{"path": "speed"}` |
| `sim.pause` | `(sim-pause)` | editor | — | — | Freeze the simulation (the transport's Pause; drops steps a sim-step still owes). No undo step. | `{}` |
| `sim.play` | `(sim-play)` | editor | — | — | Run the simulation (the transport's Play; drops steps a sim-step still owes). No undo step. | `{}` |
| `sim.set` | `(sim-set path value)` | config | `settings` | — | Set a simulation setting by reflect-path (config seam; a settled edit is one undo step). | `{"path": "speed", "value": 0.5}` |
| `sim.step` | `(sim-step n)` | editor | — | — | Pause, then advance exactly n fixed physics steps (n / timestep_hz seconds at speed 1). Returns at once; read sim-time to see the steps done. No undo step. | `{"n": 60}` |
| `sim.time` | `(sim-time)` | query | — | — | Simulated seconds on the physics clock (frozen while paused; advanced by sim-step). | `{}` |

## `tool`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `tool.box` | `(box-tool)` | editor | — | Tools/Box, B | Use the box tool: drag out a rectangle. | `{}` |
| `tool.chain` | `(chain-tool)` | editor | — | Tools/Chain | Use the chain tool: drag a path; release lays hinged links along it. | `{}` |
| `tool.circle` | `(circle-tool)` | editor | — | Tools/Circle, C | Use the circle tool: press at the centre, drag out the radius. | `{}` |
| `tool.cut` | `(cut-tool)` | editor | — | Tools/Cut, K | Use the cut tool: drag a stroke across the bodies to cut. | `{}` |
| `tool.drag` | `(drag-tool)` | editor | — | Tools/Drag, D | Use the drag tool: pull bodies with a mouse spring while playing, move them while paused. | `{}` |
| `tool.ground` | `(ground-tool)` | editor | — | Tools/Ground, G | Use the ground tool: click to place a ground plane, drag to tilt it. | `{}` |
| `tool.hinge` | `(hinge-tool)` | editor | — | Tools/Hinge, H | Use the hinge tool: click where two bodies overlap to hinge them, or on one body to pin it. | `{}` |
| `tool.polygon` | `(polygon-tool)` | editor | — | Tools/Polygon, P | Use the polygon tool: click the vertices, close at the start. | `{}` |
| `tool.prismatic` | `(prismatic-tool)` | editor | — | Tools/Prismatic, R | Use the prismatic tool: press at the anchor, drag out the slide axis. | `{}` |
| `tool.register` | `(register-tool name points src)` | editor | — | — | Add a palette tool that collects `points` clicks, then runs src once (read them with tool-x/tool-y). | `{"name": "peg", "points": 1, "src": "(spawn-circle (tool-x 0) (tool-y 0) 0.2)"}` |
| `tool.rod` | `(rod-tool)` | editor | — | Tools/Rod, L | Use the rod tool: drag from one end to the other. | `{}` |
| `tool.select` | `(select-tool)` | editor | — | Tools/Select, S | Use the select tool: click to select, drag to move, right-drag to rotate. | `{}` |
| `tool.sketch` | `(sketch-tool)` | editor | — | Tools/Sketch | Use the sketch tool: draw freehand; release makes one body of the stroke. | `{}` |
| `tool.spring` | `(spring-tool)` | editor | — | Tools/Spring, T | Use the spring tool: drag from one anchor to the other. | `{}` |
| `tool.thruster` | `(thruster-tool)` | editor | — | Tools/Thruster | Use the thruster tool: press on a body where it pushes; a click pushes up, a drag aims the push. | `{}` |
| `tool.tracer` | `(tracer-tool)` | editor | — | Tools/Tracer, N | Use the tracer tool: click a body (or empty space) to place a tracer. | `{}` |
| `tool.weld` | `(weld-tool)` | editor | — | Tools/Weld, W | Use the weld tool: click two overlapping bodies to merge them. | `{}` |

## `ui`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `ui.click` | `(ui-click at)` | editor | — | — | Click (left button) at a widget name from ui.inspect or a window point [x y]: move, press, release, one frame each, as real input. | `{"at": [640, 370]}` |
| `ui.click_button` | `(ui-click-button at button)` | editor | — | — | Click with a named button (left, middle or right) at a widget name from ui.inspect or a window point [x y]: move, press, release, one frame each. A middle click on a number field resets it to its default. | `{"at": [640, 370], "button": "middle"}` |
| `ui.drag` | `(ui-drag from to steps)` | editor | — | — | Drag (left button) from one widget name or point [x y] to another in steps frames (1-200): move, press, the steps, release, as real input; a tool commits one step on release. | `{"from": [640, 370], "to": [740, 370], "steps": 10}` |
| `ui.inspect` | `(ui-inspect prefix)` | query | — | — | List the widgets drawn last frame whose name starts with prefix ("" for all): name, area, rect [x0 y0 x1 y1] and handle [x y] in logical points. | `{"prefix": ""}` |
| `ui.key` | `(ui-key key [hold])` | editor | — | — | Press and release a key chord, written as menus show it (Ctrl+Z, Ctrl+Shift+G, Delete, Escape, F1, A): modifiers down, key down, key up, modifiers up, one frame each. hold (1-600 frames, default 1) keeps the key down that long, as a held key drives a graph key input in play. | `{"key": "Escape", "hold": 1}` |
| `ui.screenshot` | `(ui-screenshot path)` | editor | — | — | Save a PNG of the window (scene and panels) at path, once the view settles (150 ms and 3 frames at least); returns the path at once, the file follows. | `{"path": "screenshot.png"}` |
| `ui.search` | `(ui-search text)` | query | — | — | The ids of the operations F3 search lists for text, best first (a click-runnable operation: no params, or only objects, filled by the selection). | `{"text": "undo"}` |
| `ui.text` | `(ui-text text)` | editor | — | — | Type text into the focused field (click it first) in one frame; a newline presses Enter. Typed text trips no key shortcut. | `{"text": "1.5"}` |
| `ui.wheel` | `(ui-wheel at lines)` | editor | — | — | Move to a widget name or point [x y], then turn the wheel by lines (> 0: up, away from you), as real input. | `{"at": [640, 370], "lines": 1}` |

## `units`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `units.convert` | `(convert value from to)` | query | — | — | A number from one unit to another of the same dimension: (convert 0.25 "m" "cm") is 25. A quantity name stands for its SI unit, so (convert 90 "km/h" "speed") is 25 and (convert 90 "deg" "height") is an error. | `{"value": 0.25, "from": "m", "to": "cm"}` |
| `units.display` | `(display-units angle length)` | config | — | — | Show angles in deg or rad and lengths in m, cm or mm in every field (display only; workstation config, never undone). | `{"angle": "rad", "length": "cm"}` |
| `units.of` | `(unit-of unit)` | query | — | — | The SI unit symbol of what a unit measures, as quantity-unit reports it: (unit-of "km/h") is "m/s" ("" for a plain number). | `{"unit": "km/h"}` |
| `units.shown` | `(units-shown)` | query | — | — | The units the fields show, as (angle length), e.g. ("deg" "m"). | `{}` |
| `units.si` | `(q value unit)` | query | — | — | A number in a unit, in SI: (q 25 "cm") is 0.25, (q 90 "deg") is pi/2. A unit the parser does not read is an error. | `{"value": 25, "unit": "cm"}` |

## `workspace`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `workspace.label` | `(label body name)` | editor | — | — | Name a body in the workspace (a viewport tag); body is a spawn's return value. | `{"body": "00000000-0000-0000-0000-00000000000a", "name": "crate"}` |

## VM built-ins

These exist only inside a script run and are not registry operations.

| Lisp call | Seam | Doc | Why not an operation |
|---|---|---|---|
| `(action-x)` | query | World x of the point the running action was invoked at (the click); NaN outside an action. | reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none) |
| `(action-y)` | query | World y of the point the running action was invoked at (the click); NaN outside an action. | reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none) |
| `(tool-count)` | query | How many points the running script tool collected; NaN outside a tool run. | reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none) |
| `(tool-x i)` | query | World x of the running tool's i-th point (click order); NaN outside a tool run. | reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none) |
| `(tool-y i)` | query | World y of the running tool's i-th point (click order); NaN outside a tool run. | reads the running action's or tool's own invocation context, which exists only inside one VM run (a remote call or a menu row has none) |
| `(ops)` | query | List of every registered operation name. | reads the VM's own catalog; the remote surface has `gradiance.ops` for the same list |
| `(describe name)` | query | The signature and doc string of one operation, as text. | reads the VM's own catalog; the remote surface has `gradiance.ops` for the same list |
