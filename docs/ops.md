# Operations

<!-- Generated from the operation registry by `tests/it/op_conformance.rs` (`docs_ops_md_is_current`). Do not edit by hand: run `UPDATE_OPS_DOC=1 cargo test --test it docs_ops_md_is_current`. -->

Every Lisp verb, remote call, menu row and key edit is one of these 81 operations, declared once in the layer that owns it (`gradiance-op`; the recipe is in `docs/scripting.md`), or one of the 7 VM built-ins at the end. Remote: `gradiance.op {"id", "params"}` takes the id and the named params; `{"name", "args"}` takes the Lisp name and the params in call order.

Seams: **edit** lowers to an undoable intent; **config** writes a settings resource; **query** returns a value and writes nothing; **editor** writes non-authored editor state. A query's NaN ("nothing there") is `null` over remote.

## `action`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `action.register` | `(register-action label source)` | editor | — | — | Surface a named action (label + lisp source) in the editor's menus. | `{"label": "Fill", "source": "(spawn-box 0 0 1 1)"}` |

## `body`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
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

## `lighting`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `lighting.set` | `(lighting-set path value)` | config | `settings` | — | Set a lighting setting by reflect-path, e.g. "ambient", "lights[0].elevation_deg", "ssao" (a settled edit is one undo step). | `{"path": "lights[0].elevation_deg", "value": 30}` |

## `look`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `look.apply` | `(look-apply name)` | config | `settings` | — | Apply a scene look by name (studio, blueprint, poster): palette tone, overlay ink, render, lighting and scenery presets — one undo step. | `{"name": "blueprint"}` |
| `look.list` | `(looks)` | query | — | — | Every scene look's name, in menu order (the first is the default). | `{}` |

## `panel`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `panel.hide` | `(panel-hide name)` | editor | — | — | Close an editor panel by name. | `{"name": "properties"}` |
| `panel.list` | `(panels)` | query | — | — | Every editor panel in View-menu order: its name (what the panel operations take) and whether it is open. | `{}` |
| `panel.open` | `(panel-open? name)` | query | — | — | Whether an editor panel is shown, by name. | `{"name": "properties"}` |
| `panel.show` | `(panel-show name)` | editor | — | — | Open an editor panel by name, as its View-menu row does (see panel.list for the names). | `{"name": "properties"}` |
| `panel.toggle` | `(panel-toggle name)` | editor | — | — | Flip an editor panel's visibility by name. | `{"name": "properties"}` |

## `render`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `render.set` | `(render-set path value)` | config | `settings` | — | Set a render (toon) setting by reflect-path, e.g. "bands", "rim_strength", "white_point" (a settled edit is one undo step). | `{"path": "bands", "value": 3}` |

## `scene`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `scene.new` | `(new-scene)` | edit | `load-scene` | File/New scene | Replace the scene with an empty one (default environment) — one undo step, as File ▸ New scene. | `{}` |

## `scenery`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `scenery.set` | `(scenery-set path value)` | config | `settings` | — | Set a scenery setting by reflect-path, e.g. "perspective_deg", "back_offset", "back_visible" (a settled edit is one undo step). | `{"path": "perspective_deg", "value": 20}` |

## `signal`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `signal.computed` | `(defsignal name expr)` | config | `settings` | — | Declare a computed signal from an RPN expression over other signals (e.g. "t sin amp *"). | `{"name": "osc", "expr": "t sin 2 *"}` |
| `signal.get` | `(signal-get name)` | query | — | — | Current value of a named signal on the bus; NaN if unset. | `{"name": "t"}` |
| `signal.param` | `(defparam name value min max)` | config | `settings` | — | Declare a tunable signal parameter (an auto-slider) published on the bus. | `{"name": "amp", "value": 2, "min": 0, "max": 10}` |
| `signal.set` | `(signal-set name value)` | editor | — | — | Publish a named value on the signal bus (drives bindings; docs/signal-dataflow.md). | `{"name": "excitement", "value": 7}` |

## `sim`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `sim.get` | `(sim-get path)` | query | — | — | Read a simulation setting by reflect-path, e.g. "gravity.y", "speed"; NaN if the path does not resolve. | `{"path": "speed"}` |
| `sim.set` | `(sim-set path value)` | config | `settings` | — | Set a simulation setting by reflect-path (config seam; a settled edit is one undo step). | `{"path": "speed", "value": 0.5}` |

## `tool`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `tool.register` | `(register-tool name points src)` | editor | — | — | Add a palette tool that collects `points` clicks, then runs src once (read them with tool-x/tool-y). | `{"name": "peg", "points": 1, "src": "(spawn-circle (tool-x 0) (tool-y 0) 0.2)"}` |

## `ui`

| Id | Lisp call | Seam | Undo step | Menu / key | Doc | Example params |
|---|---|---|---|---|---|---|
| `ui.click` | `(ui-click at)` | editor | — | — | Click (left button) at a widget name from ui.inspect or a window point [x y]: move, press, release, one frame each, as real input. | `{"at": [640, 370]}` |
| `ui.drag` | `(ui-drag from to steps)` | editor | — | — | Drag (left button) from one widget name or point [x y] to another in steps frames (1-200): move, press, the steps, release, as real input; a tool commits one step on release. | `{"from": [640, 370], "to": [740, 370], "steps": 10}` |
| `ui.inspect` | `(ui-inspect prefix)` | query | — | — | List the widgets drawn last frame whose name starts with prefix ("" for all): name, area, rect [x0 y0 x1 y1] and handle [x y] in logical points. | `{"prefix": ""}` |
| `ui.key` | `(ui-key key)` | editor | — | — | Press and release a key chord, written as menus show it (Ctrl+Z, Ctrl+Shift+G, Delete, Escape, F1, A): modifiers down, key down, key up, modifiers up, one frame each. | `{"key": "Escape"}` |
| `ui.screenshot` | `(ui-screenshot path)` | editor | — | — | Save a PNG of the window (scene and panels) at path, once the view settles (150 ms and 3 frames at least); returns the path at once, the file follows. | `{"path": "screenshot.png"}` |
| `ui.text` | `(ui-text text)` | editor | — | — | Type text into the focused field (click it first) in one frame; a newline presses Enter. Typed text trips no key shortcut. | `{"text": "1.5"}` |
| `ui.wheel` | `(ui-wheel at lines)` | editor | — | — | Move to a widget name or point [x y], then turn the wheel by lines (> 0: up, away from you), as real input. | `{"at": [640, 370], "lines": 1}` |

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
