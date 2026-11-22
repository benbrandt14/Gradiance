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
(part of the job), **missing** (brief in `docs/briefs/S-*`), **rejected**
(owner ruling). A missing item gets a brief only when it
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
| Sketch (freehand body) | partial | `tool.sketch` (N-6). No gesture grammar: rejected (owner, 2026-11-21: no shape recognition) |
| Brush (paint/erase area) | partial | sketch tool paints; erase mode: **S-16** |
| Hinge / axle | present | `tool.hinge`, `hinge` (body-body and world pin) |
| Fixate | present | `tool.weld` |
| Spring | present | `tool.spring`, `spring`; spring from empty space pins to the world (J-1) |
| Chain / rope | present | `tool.chain`, `chain` (N-5) |
| Thruster | present | `tool.thruster`, `thruster` (N-3) |
| Tracer | present | `tool.tracer` (fade, size, pattern: `domain::tracer`) |
| Laser pen | done | `tool.laser`, `laser` (N-8): reflection and hit signal; refraction and beam colour: **S-3** |
| Gear | done | **S-5**: `tool.gear`, `spawn-gear` (rack-generated teeth, meshing by contact) |
| Texture | missing | **S-13** |
| Erase | present | Delete key, `delete` |
| Rectangle / rotate / zoom / pan (lesson era) | present | box tool, handles, camera rig |

## Top bar, menus, options

| Feature | Status | Where in Gradiance |
|---|---|---|
| Play / pause / step | present | transport, `sim-play`, `sim-pause`, `sim-step` |
| Undo / redo, undo a run | present | `undo`, `redo`; a run is one undo step |
| Gravity on/off, strength, direction | present | Settings > Simulation dial, `sim-set "gravity…"` |
| Air friction, wind | present | `SimSettings` drag/wind (N-4) |
| Air buoyancy | missing | **S-22** |
| Sim speed, frequency, iterations | present | `SimSettings.speed`, `timestep_hz`, `substeps` |
| Grid, snap | present | `GridSettings`, `SnapConfig` (Phase 1) |
| Collision layers (check/uncheck all) | present | `DepthBand` ≡ layer, Depth panel, context menu Depth |
| Material for new objects | rejected | owner, 2026-11-21 (with presets) |
| Colour palettes, skins | present | colour presets (`appearance.rs`), looks, chrome presets |
| Velocity menu: show velocity / momentum / force arrows | done | **S-2b** (per body: Appearance arrows, `set-arrows`; `debug.velocities` stays the global overlay) |
| Direction picker for velocity | done | **S-1** (#306: inspector Starting motion dial) |
| File load / save / new | present | `load-scene`, `save-scene`, `new-scene`, File menu |
| File browser with thumbnails | missing | **S-17** |
| Online scene sharing | missing | **S-18** (one-file share, no server) |
| Tutorials, lessons | declined | owner, 2026-11-19 ("not an educational tool") |
| Language choice | missing | **S-19** (string table) |
| UI scale | *unverified* | egui zoom not checked |
| Joystick | missing | **S-20** |
| Multi-touch, trackpad gestures | missing | **S-21** |

## Object context menu

| Feature | Status | Where in Gradiance |
|---|---|---|
| Clone | present | Copy/Paste (`copy-ron`, `paste-ron`), Array (`array-copies`) |
| Mirror | present | `mirror-h`, `mirror-v` (N-9) |
| Glue to background | present | `set-static`, hinge world pin |
| Loosen (drop the body's joints) | done | **S-6**: `loosen`, context menu |
| Add centre axle / centre thruster | done | **S-6**: `pin-centre`, `thruster-centre`, context menu |
| Attach tracer | present | tracer tool on a body |
| Transform into circle | done | **S-6**: `to-circle` (equal area), context menu |
| Transform into gear | done | **S-5**: context menu "Make gear", `make-gear` |
| CSG | present | `merge`, `subtract`, `intersect`, `xor`, `fillet-merge` |
| Select objects under cursor | present | context "Select from" stack |
| Liquify | present | `body.liquify`, context "Liquify" (S-11) |
| Make sponge (soft body) | present | `body.sponge`, context "Make sponge" (S-12, spring lattice) |
| Appearance: colour, opacity, border | present | `Appearance { fill (RGBA), border, emissive }`, `set-color` |
| Text on object | declined | owner rejected notes/annotations (2026-11-19); `label` names a body in the viewport |
| Geometry from image | missing | **S-14** |
| Material: density, friction, restitution | present | inspector, `set-density`, `set-friction`, `set-restitution` |
| Material: mass entry | partial | density only; mass is derived (the field was in the rejected presets brief) |
| Material: refractive index | done | **S-3** (`Optics`, `set-index`, inspector) |
| Material: attraction | present | `FieldSource` (`domain::field`, M20) |
| Material: surface velocity (conveyor) | done | **S-8**: `set-surface-speed`, inspector "surface speed" (avian contact tangent speed) |
| Material presets (wood, glass, …) | rejected | owner, 2026-11-21 |
| Velocity: set linear and angular velocity | done | **S-1** (#306: `InitialMotion`, `set-velocity`, `set-spin`) |
| Information: mass, position, speed, spin, momentum, angular momentum, kinetic energy | present | quantity catalog (`quantity`), signal sources, Probe panel |
| Information: area, moment of inertia, potential energy | done | **S-2** (catalog rows `area`, `inertia`, `potential-energy`) |
| Collision layers | present | Depth panel, context Depth |
| Plot vs time | present | Plot panel, signal history |
| Plot y vs x (phase plot) | done | **S-2** (Plot panel `x:` choice) |
| Plot acceleration | done | **S-2** (catalog row `acceleration`; force: `ContactForce`, `JointForce`) |
| Keystroke control | present | `defkey` (N-2) driving N-1 sinks (motor speed, thrust, …) |
| Script menu (every property, editable) | partial | console + `reflect_grid`; settings get a grid, bodies get the hand inspector. Kept: the inspector is the edit seam |

## Joints and nodes

| Feature | Status | Where in Gradiance |
|---|---|---|
| Hinge motor: speed, torque, reverse | present | `MotorConfig` (`domain::joint`), joint inspector |
| Hinge brake | done | **S-6**: `brake`, joint context menu "Brake motor" |
| Hinge limits | present | joint inspector limits |
| Break limit (rope, joint) | present | `JointCommon.break_force`, `JointLoad` tint (N-9) |
| Spring strength, damping, length | present | spring frequency, damping ratio, rest length |
| Thruster force, always-on or key | present | `Thruster`, N-1 `Thrust` sink + `defkey` |
| Laser colour, fade distance | partial | `range` = fade distance; colour: **S-3** |
| Tracer colour, fade | present | `Tracer.fade_secs`, `TracerColor` sink |

## Water and optics

| Feature | Status | Where in Gradiance |
|---|---|---|
| Particle water, add water | present | particles (S-10, `docs/particle-fluid-decision.md`); water tool, `water.fill`, `water.drain` (S-11); `Fluid` region (N-7) stays |
| Refraction, total internal reflection | missing | **S-3** |
| Mirror (index ∞) | present | every solid body reflects (N-8) |
| Dispersion, white light | missing | **S-15** (after S-3) |

## Scripting

| Feature | Status | Where in Gradiance |
|---|---|---|
| Script language + console | present | Steel Lisp, REPL console, `--script` |
| Spawn from script | present | `spawn-*`, `chain`, `sketch`, `hinge`, … |
| Read/write position, angle | present | `body-x`, `body-y`, `body-rot`, `place` |
| Read/write velocity | done | read via `quantity`; the starting value written by `set-velocity`/`set-spin` (**S-1**) |
| Per-frame update / post-step | present | computed signals (`defsignal`, Tier-B kernel): the VM never runs per frame by rule |
| On collide | done | **S-7**: the `hit` pulse (signal source `HitPulse`, quantity `hit`), remove on contact; no script callback (the VM never runs per frame) |
| On key | present | `defkey` |
| On laser hit | present | laser bus signal `<name>` (N-8) |
| On spawn | declined | spawn is a command; a script runs it already |
| Time to live | done | **S-7**: `set-lifetime`, inspector "remove after" |
| Entity by id | present | verbs take an index or a `StableId` |
| Sim time, running | present | `sim-time` |
| Camera zoom / pan from script | done | **S-9**: `camera-look x y zoom` |
| Mouse position | partial | `action-x`, `action-y`, tool points |
| File write | present | `save-scene`, `signal-export` |
| Random numbers | *unverified* | Steel built-ins not checked |

## Camera

| Feature | Status | Where in Gradiance |
|---|---|---|
| Zoom, pan | present | camera rig, `camera-set` (feel only) |
| Follow a body | done | **S-9**: `camera-follow`, View > Follow selection |
| Zoom to fit / to selection | done | **S-9**: `camera-frame`, View > Frame selection (`.`) and Frame all |

## Rejected or declined

Owner ruling 2026-11-21: "Reject material presets and include the
others." Every item the first audit declined now has a brief (S-10 ..
S-22), except these three, which the owner rejected earlier:

- **Material presets, default material** (owner, 2026-11-21).
- **Sketch gesture grammar** (owner, 2026-11-21: no freehand shape
  recognition, "this should be a more precise tool than that").
- **Text notes, tutorials and examples** (owner, 2026-11-19: "this isn't
  an educational tool").

**Size warning.** S-10 (particle fluid) is a spike with a decision note,
because body coupling is unverified. S-11 waits for it. S-12 builds a soft
body from existing springs, not a new solver.
