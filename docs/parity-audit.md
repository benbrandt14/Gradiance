# Sandbox parity audit (2026-11-20)

Checks every feature and menu option of the reference 2D physics sandbox
(the one Gradiance takes its direction from) against Gradiance `main`
(`523ed82`). Features are described generically: no names, art or
assets come from the reference.

**Sources** (public): the encyclopedia entry for the reference app
(2.2.x tool list and context menu), a 2010 lesson book (tools, optics,
water), two release changelogs (1.8.0 to 2.0.2), and a published script
configuration file (event names, scene functions). Where a source and the
current reference disagree, the newer source wins. Items marked
*unverified* were not checked in code.

**Status:** **present** (same job, maybe different shape), **partial**
(part of the job), **missing** (brief in `docs/briefs/S-*`), **declined**
(not planned, with the reason). A missing item gets a brief only when it
reuses an existing seam at a bounded cost; the rest go to the roadmap
Horizon or are declined.

Not duplicated here: the Phase Q follow-ups (aero torque, chain pin
jitter, water surface, laser canvas block, `signal.bind`).

## Tools

| Feature | Status | Where in Gradiance |
|---|---|---|
| Box, circle, polygon | present | `tool.box` / `tool.circle` / `tool.polygon`; `spawn-box`, `spawn-circle` |
| Infinite plane | present | `tool.ground`, `spawn-ground` (half-plane) |
| Move / rotate / scale | present | select tool + handles (`tools/handles.rs`), `place`, `scale` |
| Drag (force in play) | present | `tool.drag` (mouse spring, `physics::grab`) |
| Knife / cut | present | `tool.cut`, `cut` |
| Sketch (freehand body) | partial | `tool.sketch` (N-6). No gesture grammar (draw a hinge, a spring, play/undo by stroke): tools and keys do those jobs; declined below |
| Brush (paint/erase area) | partial | sketch tool + CSG `subtract`. No erase-by-brush mode; declined below |
| Hinge / axle | present | `tool.hinge`, `hinge` (body-body and world pin) |
| Fixate | present | `tool.weld` |
| Spring | present | `tool.spring`, `spring`; spring from empty space pins to the world (J-1) |
| Chain / rope | present | `tool.chain`, `chain` (N-5) |
| Thruster | present | `tool.thruster`, `thruster` (N-3) |
| Tracer | present | `tool.tracer` (fade, size, pattern: `domain::tracer`) |
| Laser pen | partial | `tool.laser`, `laser` (N-8): reflection and hit signal. No refraction, no beam colour: **S-3** |
| Gear | missing | **S-5** (gear outline as geometry, meshing by contact) |
| Texture | declined | see Declined |
| Erase | present | Delete key, `delete` |
| Rectangle / rotate / zoom / pan (lesson era) | present | box tool, handles, camera rig |

## Top bar, menus, options

| Feature | Status | Where in Gradiance |
|---|---|---|
| Play / pause / step | present | transport, `sim-play`, `sim-pause`, `sim-step` |
| Undo / redo, undo a run | present | `undo`, `redo`; a run is one undo step |
| Gravity on/off, strength, direction | present | Settings > Simulation dial, `sim-set "gravity…"` |
| Air friction, wind | present | `SimSettings` drag/wind (N-4) |
| Air buoyancy | declined | see Declined |
| Sim speed, frequency, iterations | present | `SimSettings.speed`, `timestep_hz`, `substeps` |
| Grid, snap | present | `GridSettings`, `SnapConfig` (Phase 1) |
| Collision layers (check/uncheck all) | present | `DepthBand` ≡ layer, Depth panel, context menu Depth |
| Material for new objects | missing | **S-4** |
| Colour palettes, skins | present | colour presets (`appearance.rs`), looks, chrome presets |
| Velocity menu: show velocity / momentum / force arrows | partial | `debug.velocities` (global only). Per body, plus momentum and force: **S-2** |
| Direction picker for velocity | missing | **S-1** (gravity already has one) |
| File load / save / new | present | `load-scene`, `save-scene`, `new-scene`, File menu |
| File browser with thumbnails | declined | see Declined |
| Online scene sharing | declined | see Declined |
| Tutorials, lessons | declined | owner, 2026-11-19 ("not an educational tool") |
| Language choice | declined | see Declined |
| UI scale | *unverified* | egui zoom not checked |
| Joystick / multi-touch input | declined | see Declined |

## Object context menu

| Feature | Status | Where in Gradiance |
|---|---|---|
| Clone | present | Copy/Paste (`copy-ron`, `paste-ron`), Array (`array-copies`) |
| Mirror | present | `mirror-h`, `mirror-v` (N-9) |
| Glue to background | present | `set-static`, hinge world pin |
| Loosen (drop the body's joints) | missing | **S-6** |
| Add centre axle / centre thruster | partial | tools place them by click, no one-click centre action: **S-6** |
| Attach tracer | present | tracer tool on a body |
| Transform into circle | missing | **S-6** |
| Transform into gear | missing | **S-5** |
| CSG | present | `merge`, `subtract`, `intersect`, `xor`, `fillet-merge` |
| Select objects under cursor | present | context "Select from" stack |
| Liquify | declined | needs particle water, see Declined |
| Make sponge (soft body) | declined | see Declined |
| Appearance: colour, opacity, border | present | `Appearance { fill (RGBA), border, emissive }`, `set-color` |
| Text on object | declined | owner rejected notes/annotations (2026-11-19); `label` names a body in the viewport |
| Geometry from image | declined | see Declined |
| Material: density, friction, restitution | present | inspector, `set-density`, `set-friction`, `set-restitution` |
| Material: mass entry | partial | density only; mass is derived. Mass field: **S-4** |
| Material: refractive index | missing | **S-3** |
| Material: attraction | present | `FieldSource` (`domain::field`, M20) |
| Material: surface velocity (conveyor) | missing | **S-8** |
| Material presets (wood, glass, …) | missing | **S-4** |
| Velocity: set linear and angular velocity | missing | **S-1** (velocity is not authored today) |
| Information: mass, position, speed, spin, momentum, angular momentum, kinetic energy | present | quantity catalog (`quantity`), signal sources, Probe panel |
| Information: area, moment of inertia, potential energy | missing | **S-2** |
| Collision layers | present | Depth panel, context Depth |
| Plot vs time | present | Plot panel, signal history |
| Plot y vs x (phase plot) | missing | **S-2** |
| Plot acceleration | missing | **S-2** (force: `ContactForce`, `JointForce` present) |
| Keystroke control | present | `defkey` (N-2) driving N-1 sinks (motor speed, thrust, …) |
| Script menu (every property, editable) | partial | console + `reflect_grid`; settings get a grid, bodies get the hand inspector. Kept: the inspector is the edit seam |

## Joints and nodes

| Feature | Status | Where in Gradiance |
|---|---|---|
| Hinge motor: speed, torque, reverse | present | `MotorConfig` (`domain::joint`), joint inspector |
| Hinge brake | partial | motor at 0 speed with a torque cap holds; no one-click brake: **S-6** |
| Hinge limits | present | joint inspector limits |
| Break limit (rope, joint) | present | `JointCommon.break_force`, `JointLoad` tint (N-9) |
| Spring strength, damping, length | present | spring frequency, damping ratio, rest length |
| Thruster force, always-on or key | present | `Thruster`, N-1 `Thrust` sink + `defkey` |
| Laser colour, fade distance | partial | `range` = fade distance; colour: **S-3** |
| Tracer colour, fade | present | `Tracer.fade_secs`, `TracerColor` sink |

## Water and optics

| Feature | Status | Where in Gradiance |
|---|---|---|
| Particle water, add water | declined | `Fluid` region (N-7) floats bodies; particle water: see Declined |
| Refraction, total internal reflection | missing | **S-3** |
| Mirror (index ∞) | present | every solid body reflects (N-8) |
| Dispersion, white light | declined | see Declined |

## Scripting

| Feature | Status | Where in Gradiance |
|---|---|---|
| Script language + console | present | Steel Lisp, REPL console, `--script` |
| Spawn from script | present | `spawn-*`, `chain`, `sketch`, `hinge`, … |
| Read/write position, angle | present | `body-x`, `body-y`, `body-rot`, `place` |
| Read/write velocity | partial | read via `quantity`; write: **S-1** |
| Per-frame update / post-step | present | computed signals (`defsignal`, Tier-B kernel): the VM never runs per frame by rule |
| On collide | partial | `ContactCount`, `ContactForce` sources; no edge event, no action on hit: **S-7** |
| On key | present | `defkey` |
| On laser hit | present | laser bus signal `<name>` (N-8) |
| On spawn | declined | spawn is a command; a script runs it already |
| Time to live | missing | **S-7** |
| Entity by id | present | verbs take an index or a `StableId` |
| Sim time, running | present | `sim-time` |
| Camera zoom / pan from script | missing | **S-9** |
| Mouse position | partial | `action-x`, `action-y`, tool points |
| File write | present | `save-scene`, `signal-export` |
| Random numbers | *unverified* | Steel built-ins not checked |

## Camera

| Feature | Status | Where in Gradiance |
|---|---|---|
| Zoom, pan | present | camera rig, `camera-set` (feel only) |
| Follow a body | missing | **S-9** |
| Zoom to fit / to selection | missing | **S-9** |

## Declined, with reasons

- **Particle water, liquify, add water.** The planner did not recommend it
  on 2026-11-19 (roadmap § Nonlinear dynamics). It needs a particle solver
  coupled to avian, a new derived store and a renderer. The kernel spike
  de-risks the maths, not the coupling. Kept on the roadmap Horizon.
- **Soft bodies (sponge).** A spring lattice is buildable today from
  `chain`/`spring` verbs in a script. A real soft body is a new solver.
  Horizon.
- **Textures and geometry from image.** Conflicts with the owner's toon
  look direction (2026-11-19). Image geometry needs a sampled-field SDF
  leaf, because `polygonize` is the single discretization point. Horizon.
- **Dispersion and white light.** Follows S-3 only if refraction proves
  useful. Not briefed.
- **Sketch gesture grammar, brush erase.** Tools, keys and CSG cover the
  same jobs. A gesture recogniser is a large input surface for low gain.
- **File browser thumbnails, online sharing, language choice, joystick,
  multi-touch, tutorials, text notes.** Product scope the owner rejected
  (notes, examples) or that does not fit a desktop engineering sandbox.
- **Air buoyancy.** Air density already enters drag. A buoyant force of
  air on a 2D body is below the solver noise for normal densities.
