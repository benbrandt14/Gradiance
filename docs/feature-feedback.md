# Feature checklist & feedback

Every user-facing behavior, grouped by area, with the contract it is
supposed to meet. Fill in the **Feedback** column (works / broken how /
wrong feel / missing) — item numbers make replies easy to itemize.
Where a reported issue is already known it is noted inline.

The **Debug tab** (Settings → Debug) exists to ground this feedback:
overlays for colliders/AABBs/origins/joint anchors/velocities, live
internals (fps, counts, undo depth, tool, snap state, selection ids,
primary shape tree), and an authored-joints readout showing exactly what
kind/anchors each joint has. Hold **Tab** for the depth peek (tilted
view of the 2.5D extrusion). **F12** writes a scene snapshot you can
attach to any item.

## 1. Creation tools

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 1.1 | Box (B) | Drag corner-to-corner; snapped endpoints; min side 1px | |
| 1.2 | Circle (C) | Drag center-to-radius | |
| 1.3 | Polygon (P) | Click vertices; close by clicking near start (8px) or Enter; Escape cancels | |
| 1.4 | Ground (G) | Click places infinite half-plane; drag tilts it; collides as a true infinite plane | |
| 1.5 | New-body color | Hue follows front-most layer bit (bit·30°) | |

## 2. Selection & manipulation (S)

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 2.1 | Click select | Topmost body (front layer first, ground last); selecting a grouped body selects the whole group | |
| 2.2 | Shift+click | Toggles body (or its whole group) in/out of the selection — *reported: "uneven usage with ctrl add/remove"; current contract is shift=toggle, ctrl=duplicate-drag. Say which keys you expect from Algodoo* | |
| 2.3 | Box select | Drag on empty canvas; Shift keeps existing selection | |
| 2.4 | Lasso | Alt+drag freeform loop; selects bodies whose center is inside | |
| 2.5 | Move | Drag selected body; X/Y axis locks; Shift dominant-axis; snapped | |
| 2.6 | Rotate | Right-drag on selected body about selection centroid; Ctrl quantizes angle. **Fixed this round:** a 4px deadzone so right-*click* opens the context menu instead of arming rotation | |
| 2.7 | Scale handles | 8 handles on the selection box; corners scale both axes, edges one; anchored at opposite handle; F toggles global/local frame | |
| 2.8 | Ctrl+drag duplicate | Ghost preview, one undo step, internal joints cloned | |
| 2.9 | Group / Ungroup | **New:** Ctrl+G / Ctrl+Shift+G (also in context menu — see 5) | |
| 2.10 | Delete | Del/Backspace; joints referencing deleted bodies cascade | |
| 2.11 | Drag tool (D) | Playing: mouse spring (not undoable). Paused: kinematic hold, one undo step | |

## 3. Snapping & grids

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 3.1 | Object snaps | Vertex > midpoint > center > edge by proximity, beat the grid; per-source toggles in settings | |
| 3.2 | Snap exclusion | Dragged bodies never snap to themselves | |
| 3.3 | Grid systems | Cartesian / isometric / polar; movable origin, rotatable basis; adaptive display density | |
| 3.4 | Snap glyphs | Distinct glyph per snap kind at the cursor | |

## 4. Joints & constraints

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 4.1 | Hinge (H) | Click: two topmost bodies at the point get a revolute; one body → world pin. Free rotation, no limits/motor by default. *Reported: "same behavior as weld" — a headless contrast test (swing vs rigid) passes, so check the Debug tab joints readout in your scene: does the entry say `Hinge`, and what bodies did it bind? A hinge between two bodies resting on the ground can look rigid* | |
| 4.2 | Weld (W) | Same gesture; rigid fixed joint | |
| 4.3 | Slider (R) | Press anchor, drag axis, release; short drag = body local X. *Reported: works but unstable — describe the instability (jitter? drift? explodes?) + F12 snapshot* | |
| 4.4 | World pins | Single-body joints anchor to a static, collider-less pin (pin explosions unrepresentable) | |
| 4.5 | Motors | Inspector-editable on hinges/sliders; oscillate mode reverses at limits | |
| 4.6 | Connected collision | Joined bodies don't collide by default (`collide_connected` flag) | |
| 4.7 | Joint glyphs | Ring = hinge, square = weld, axis = slider, orange-red = world pin | |

## 5. Context menu (right-click) — **fixed this round**

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 5.1 | Opens on right click | On bodies or empty canvas (was swallowed by the rotate gesture) | |
| 5.2 | Group / Ungroup | On the current selection | |
| 5.3 | Select-from stack | Lists overlapping bodies under the click, topmost first | |
| 5.4 | Layer assignment | Buttons 0–7 move the selection to a layer | |
| 5.5 | Isolate collisions | Moves selection to a free bit that ignores itself (members pass through each other, still hit everything else) | |
| 5.6 | Reset layers | Back to layer 0, all filters on | |

## 6. Inspector & settings UI

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
| 7.1 | Cut stroke | Drag a line; every crossed body is subtracted; one undo step | |
| 7.2 | Severing | Disconnected remainders become independent bodies, recentered | |
| 7.3 | Analytic notches | Non-severing cuts on boxes/circles keep exact analytic geometry (inspector shows "CSG shape") | |
| 7.4 | Joint reattach | Joints follow the piece containing their anchor, else delete; undo restores | |
| 7.5 | Piece velocity | *Known gap:* pieces spawn at rest (no v + ω×r inheritance yet) | |

## 8. Simulation

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 8.1 | Play/pause | Space; paused editing is exact (no drift) | |
| 8.2 | Sim settings | Gravity vector + speed multiplier apply live | |
| 8.3 | Stability | *You reported "largely stable" — note any blow-ups + F12 snapshot* | |

## 9. Rendering — **reworked this round**

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 9.1 | Backdrop + shadows | **New:** a matte wall behind the deepest layer catches cast shadows — this was the missing piece behind "flat-lit / no lighting": drop shadows previously fell into the void | |
| 9.2 | Toon banding | Bands follow real illumination (incl. shadows); `white_point` in Rendering settings tunes saturation — if everything still reads as one flat band, lower it live | |
| 9.3 | Depth peek | **New:** hold Tab to tilt the camera and see the extrusion/layer depth (view-only; cursor inert). "Everything still 2D" head-on is *expected* for an orthographic front view — depth shows via shadows and the peek. If you want a permanently tilted editing view, say so (requires ray-plane picking work) | |
| 9.4 | Layer depth | Extrusion depth = occupied layer bits × 10 (visible in the peek) | |
| 9.5 | Rim light | Rendering settings `rim_strength` | |

## 10. Camera

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 10.1 | Pan | Right/middle drag on empty space, arrow keys | |
| 10.2 | Zoom | Wheel, anchored at cursor | |

## 11. Persistence

| # | Feature | Contract | Feedback |
|---|---|---|---|
| 11.1 | Save/Load | Ctrl+S / Ctrl+Shift+S / Ctrl+O, RON files, environment settings included | |
| 11.2 | Undoable load | Loading a scene is one undo step | |
| 11.3 | Snapshots | F12 → `snapshots/gradiance-<ts>.ron`; `gradiance <file>` reproduces it | |

## 12. Known deferred features (M12+ planning)

Springs/dampers, cams, planar contact, magnetism (SDF force fields),
breaking limits/backlash, piece velocity inheritance, curve pickers,
symbolic/equation input, tracers, scripting, permanently tilted editing
camera, smooth-union modeling tools.
