# Feature checklist & feedback

Every user-facing behavior, grouped by area, with the contract it is
supposed to meet. Fill in the **Feedback** column (works / broken how /
wrong feel / missing) — item numbers make replies easy to itemize.
Where a reported issue is already known it is noted inline.

> **Status (M12):** feedback triaged — immediate fixes landed (joint rest
> frames from your slider snapshot, cut severs-only, selection
> containment/ground exclusion, group-id remap on duplicate, lasso on
> Ctrl+drag, random body colors, grid-behind/selection-in-front, CAD
> orbit camera with ray-plane picking replacing the broken Tab peek —
> **middle-drag orbits, Home returns to 2D**). Everything else is
> scheduled in `docs/roadmap.md` with your item numbers.

The **Debug tab** (Settings → Debug) exists to ground this feedback:
overlays for colliders/AABBs/origins/joint anchors/velocities, live
internals (fps, counts, undo depth, tool, snap state, selection ids,
primary shape tree), and an authored-joints readout showing exactly what
kind/anchors each joint has. **Middle-drag orbits** the 3D view
(`Home` glides back to 2D). **F12** writes a scene snapshot you can
attach to any item.

## 1. Creation tools

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 1.1 | Box (B) | Drag corner-to-corner; snapped endpoints; min side 1px | Ghost ok, shift-hold on select injects invalid position offsets | 
| 1.2 | Circle (C) | Drag center-to-radius | | All ok, visualize complete circle with a line from center to edge (old school algodoo, same as ghost helper) | 
| 1.3 | Polygon (P) | Click vertices; close by clicking near start (8px) or Enter; Escape cancels | Ok, ghost image hides behind grid. ( grid should be behind everything ) | 
| 1.4 | Ground (G) | Click places infinite half-plane; drag tilts it; collides as a true infinite plane | (!) Not viewed as an infinite plane ( has obvious thickness ), is frequently erroniously included in selection, should have angle-quantization option during creation | 
| 1.5 | New-body color | Hue follows front-most layer bit (bit·30°) | All new bodies are red. Overall color scheme is not themed and somewhat dark, color picker should be quantized, borders should be colored as well ( default border outline dark gray ), should have context menu to set border color and transparency as well as color, or to assign random colors within a grouped selection |

## 2. Selection & manipulation (S)

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 2.1 | Click select | Topmost body (front layer first, ground last); selecting a grouped body selects the whole group | seems ok, right click induces some unexpected rotation when trying to get context menu, overall feel does not match algodoo yet (which cleanly defaulted to selections even when the select tool was not active)|
| 2.2 | Shift+click | Toggles body (or its whole group) in/out of the selection — *reported: "uneven usage with ctrl add/remove"; current contract is shift=toggle, ctrl=duplicate-drag. Say which keys you expect from Algodoo* | Unexpected behavior with plane tool ( selection should not include partials ), unexpected behavior when holding shift then dragging a group, click-drag should immediately kinematic move in select mode. Single click should show a selection outline that is visible above object outlines or grids. |
| 2.3 | Box select | Drag on empty canvas; Shift keeps existing selection | Under some conditions (after repeated operations) group selection behavior deteriorates snapshots/gradiance-1783343319.ron |
| 2.4 | Lasso | Alt+drag freeform loop; selects bodies whose center is inside | No lasso tool is seen, alt+drag just moves my window (desktop feature, not game), should not select partials|
| 2.5 | Move | Drag selected body; X/Y axis locks; Shift dominant-axis; snapped | Drag tool & select+drag should perform a static move during pause mode, issue above possibly related to snapping, snapping points should obviously relate to the grid, and be robust to grid level for all zooms. The dominant axis should match the basis of whichever grid is active. |
| 2.6 | Rotate | Right-drag on selected body about selection centroid; Ctrl quantizes angle. **Fixed this round:** a 4px deadzone so right-*click* opens the context menu instead of arming rotation | Is ok for just the select tool, should dynamically rotate when in play mode (center non fixed, will lift opposing edge), debug contact points and forces should be optionally visualized, right click needs more algodoo-like behavior across multiple tools and modes ( since drag and rotate are common actions ) |
| 2.7 | Scale handles | 8 handles on the selection box; corners scale both axes, edges one; anchored at opposite handle; F toggles global/local frame | This works well, needs a shift action for scaling with locked aspect ratio |
| 2.8 | Ctrl+drag duplicate | Ghost preview, one undo step, internal joints cloned | joints should be treated as a separate selectable entity, they should only be cloned if they're part of the selection, they should be right-clickable to change settings ( most of the inspector should be a context menu, inspector should be a pop-out box from a context menu command )|
| 2.9 | Group / Ungroup | **New:** Ctrl+G / Ctrl+Shift+G (also in context menu — see 5) | groups should be heirarchical, ie ungroup(group(group(A B C), D)) should keep group(A B C) |
| 2.10 | Delete | Del/Backspace; joints referencing deleted bodies cascade | |
| 2.11 | Drag tool (D) | Playing: mouse spring (not undoable). Paused: kinematic hold, one undo step | |

## 3. Snapping & grids

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 3.1 | Object snaps | Vertex > midpoint > center > edge by proximity, beat the grid; per-source toggles in settings | largely good, but there is not much reason to snap right now. Alternate shape constructors ( like a 3 point box, or tangent circle ) might benefit from this more, would like easier alignment when moving objects in pause mode (almost like a very lightweight version of CAD assemblies, mostly just to make objects co-linear when dragged together), should be configurable in snapping menu. Grid has more overall polish issues compared to other features. Should lightly snap to collinear on move/create, should optionally snap to centerlines|
| 3.2 | Snap exclusion | Dragged bodies never snap to themselves | ok |
| 3.3 | Grid systems | Cartesian / isometric / polar; movable origin, rotatable basis; adaptive display density | snaps don't always align with grid, need major and minor grid, Polygon tool should have curved sides in curvilinear grid systems (generally should abstract most operations so features in curvilinear coordinates just work) |
| 3.4 | Snap glyphs | Distinct glyph per snap kind at the cursor | seem to be ok, snap should never be enabled if the grid is not visible, circle snap glyph rapidly changes between type, tangent should at least have a glyph here|

## 4. Joints & constraints

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 4.1 | Hinge (H) | Click: two topmost bodies at the point get a revolute; one body → world pin. Free rotation, no limits/motor by default. *Reported: "same behavior as weld" — a headless contrast test (swing vs rigid) passes, so check the Debug tab joints readout in your scene: does the entry say `Hinge`, and what bodies did it bind? A hinge between two bodies resting on the ground can look rigid* | no change, weld still matches joint. joint not selectable, needed for context menu access to configuration, should never change body-position on resize, should be moveable via tool in pause mode, should visually indicate it's state (motor direction/torque). Need better debug utilities to show constraint issues|
| 4.2 | Weld (W) | Same gesture; rigid fixed joint | weld should not even be a constraint essentially, it should make an object static, or it should make 2 objects into 1 object, weld is not selectable|
| 4.3 | Slider (R) | Press anchor, drag axis, release; short drag = body local X. *Reported: works but unstable — describe the instability (jitter? drift? explodes?) + F12 snapshot* | slider is set, but does not prevent rotation, body starts to move then jitters and explodes see snapshots/gradiance-1783344618.ron|
| 4.4 | World pins | Single-body joints anchor to a static, collider-less pin (pin explosions unrepresentable) | unsure, seems ok|
| 4.5 | Motors | Inspector-editable on hinges/sliders; oscillate mode reverses at limits | cannot access via menu |
| 4.6 | Connected collision | Joined bodies don't collide by default (`collide_connected` flag) | ok, a chain of joints still works ( collides non-parents by default, unless deselected )|
| 4.7 | Joint glyphs | Ring = hinge, square = weld, axis = slider, orange-red = world pin | largely ok, is overall too small ( does not scale with zoom correctly ) and should use grey symbols with better outlines, possibly sprites |

## 5. Context menu (right-click) — **fixed this round**

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 5.1 | Opens on right click | On bodies or empty canvas (was swallowed by the rotate gesture) | rotate is still slightly too sensitive |
| 5.2 | Group / Ungroup | On the current selection | ok |
| 5.3 | Select-from stack | Lists overlapping bodies under the click, topmost first | need better "move up" "move down" "move to front" "move to back" options for selections. Add features similar to powerpoint orient operations (match size, align horizontal, distribute vertical, etc) with added ability to do this in the future for not just position, but any attribute within a group. Ie, box select items, see min/max range on mass slider, right click slider and select "logarithmic distribute" and it log spaces only that attribute for the selection|
| 5.4 | Layer assignment | Buttons 0–7 move the selection to a layer | Needs refinement, don't want everything to be keyboard shortcuts, need nicer UI for collision layer set visualization |
| 5.5 | Isolate collisions | Moves selection to a free bit that ignores itself (members pass through each other, still hit everything else) | Appears to work, should be called "no self-collisions"|
| 5.6 | Reset layers | Back to layer 0, all filters on | don't see this option|

## 6. Inspector & settings UI ( User note: No immediate additional issues, will return for finer pass after )

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 6.1 | Physics section | Body kind, density, friction, restitution, gravity scale, sensor, rotation lock; multi-select batch edit = one undo step | |
| 6.2 | Shape section | Box w/h, circle radius editable; polygons/CSG summarized | |
| 6.3 | Color | RGBA picker | |
| 6.4 | Layer bits | Checkboxes 0–7 | |
| 6.5 | Precision widgets | Commit-on-release (one undo step), scientific notation input, middle-click resets to default | |
| 6.6 | Settings tabs | Simulation / Grid & Snap / Rendering / **Debug (new)** — reflection-driven, new fields appear automatically | |
| 6.7 | Keyboard capture | **New:** typing in UI fields no longer triggers tool hotkeys; Ctrl chords no longer also switch tools | |

## 7. CSG cutting (K)

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 7.1 | Cut stroke | Drag a line; every crossed body is subtracted; one undo step | amazingly works|
| 7.2 | Severing | Disconnected remainders become independent bodies, recentered | ok |
| 7.3 | Analytic notches | Non-severing cuts on boxes/circles keep exact analytic geometry (inspector shows "CSG shape") | cut should only occur if it is severing, separate CSG feature set should come from selecting a tool body and applying boolean operations (join,subtract,xor,etc) via context menu. We don't want microscopic thin features. |
| 7.4 | Joint reattach | Joints follow the piece containing their anchor, else delete; undo restores | notionally works, needs to be tested in depth though |
| 7.5 | Piece velocity | *Known gap:* pieces spawn at rest (no v + ω×r inheritance yet) | notionally correct |

## 8. Simulation

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 8.1 | Play/pause | Space; paused editing is exact (no drift) | ok |
| 8.2 | Sim settings | Gravity vector + speed multiplier apply live | ok, needs UI refinement (click+drag to scrub, widget for setting direction, more options) |
| 8.3 | Stability | *You reported "largely stable" — note any blow-ups + F12 snapshot* | no blow ups found, should be able to specify engine details like timesteps/substeps and debug view substeps |

## 9. Rendering — **reworked this round**

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 9.1 | Backdrop + shadows | **New:** a matte wall behind the deepest layer catches cast shadows — this was the missing piece behind "flat-lit / no lighting": drop shadows previously fell into the void | |
| 9.2 | Toon banding | Bands follow real illumination (incl. shadows); `white_point` in Rendering settings tunes saturation — if everything still reads as one flat band, lower it live | |
| 9.3 | Depth peek | **New:** hold Tab to tilt the camera and see the extrusion/layer depth (view-only; cursor inert). "Everything still 2D" head-on is *expected* for an orthographic front view — depth shows via shadows and the peek. If you want a permanently tilted editing view, say so (requires ray-plane picking work) | tab doesn't work, everything is 2d, should have actual 3d camera controls that fluidly reset to 2d ( think CAD UX ) with ray-plane picking and other 3d lighting. Objects should have possible emmissivity, all the rendering needs substantial work, but the 2d case is nominally correct. Would like better ambient occlusion and corner shadows for better clay-like matte rendering|
| 9.4 | Layer depth | Extrusion depth = occupied layer bits × 10 (visible in the peek) | no visible extrusion |
| 9.5 | Rim light | Rendering settings `rim_strength` | setting exists, does nothing |

## 10. Camera

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 10.1 | Pan | Right/middle drag on empty space, arrow keys | ok |
| 10.2 | Zoom | Wheel, anchored at cursor | to sensitive, needs config options |

## 11. Persistence

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 11.1 | Save/Load | Ctrl+S / Ctrl+Shift+S / Ctrl+O, RON files, environment settings included | seems to largely work, but had a crash when loading a save with a partial cut "2026-07-06T13:51:35.650055Z  WARN gradiance::command: command failed name="Cut" error=command had no effect"|
| 11.2 | Undoable load | Loading a scene is one undo step | |
| 11.3 | Snapshots | F12 → `snapshots/gradiance-<ts>.ron`; `gradiance <file>` reproduces it | |

## 12. Known deferred features (M12+ planning)

Springs/dampers, cams, planar contact, magnetism (SDF force fields),
breaking limits/backlash, piece velocity inheritance, curve pickers,
symbolic/equation input, tracers, scripting, permanently tilted editing
camera, smooth-union modeling tools.
