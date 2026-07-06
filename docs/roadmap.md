# Roadmap

Numbers in parentheses reference `docs/feature-feedback.md` items.
Updated after the M12 feedback round.

## Addressed in M12 (this round)

- **Joint rest frames** (4.3, partially 4.2): joints capture creation-time
  rotations; sliders lock rotation at the authored angle instead of
  snapping upright and exploding (regression test distilled from
  `snapshots/gradiance-1783344618.ron`); welds hold rotated pairs at their
  authored relative angle.
- **Cut severs-only** (7.3): partial strokes are rejected outright — no
  notches, no thin features, no undo entry.
- **CAD camera** (9.3, 9.4 foundation): middle-drag orbits, `Home` glides
  back to 2D, and picking is ray/plane — the tilted view is a real editing
  view (replaces the broken Tab peek). Extrusion and rim light are only
  visible when orbited; head-on stays nominally 2D by design.
- **Selection** (2.2, 2.3, 2.4, 1.4): box select takes only fully-contained
  bodies and never the ground; lasso moved to Ctrl+drag (Alt is
  OS-reserved; Alt still works where free) and skips the ground;
  duplicated/arrayed groups get fresh group ids (the "deteriorating group
  selection" bug); rotate deadzone widened (5.1).
- **Ground** (1.4 partial): Ctrl quantizes the tilt during creation;
  excluded from box/lasso selection.
- **Colors** (1.5 partial): new bodies get a random pleasant hue derived
  from their id (no more all-red).
- **Layering** (1.3, 2.2): grid renders behind bodies and ghosts;
  selection outlines render in front of everything.
- **Misc**: circle center-to-edge radius line (1.2); joint glyphs scale
  with zoom (4.7 partial); gentler zoom (10.2 partial); context-menu
  renames "No self-collisions" / "Reset collision layers" (5.5, 5.6).

## M13 — Interaction & selection feel (Algodoo parity)

- Selection works from every tool (click falls through to select) (2.1)
- Shift-drag / modifier semantics rework; no gesture dead-ends (1.1, 2.2)
- Play-mode right-drag applies torque (dynamic rotate, non-fixed pivot) (2.6)
- Shift = aspect-locked scaling (2.7)
- **Hierarchical groups**: `ungroup(group(group(A,B,C), D))` keeps the
  inner group (2.9)
- Z-order operations: move up/down/front/back (5.3)
- Align/distribute (PowerPoint-style), later generalized to *any
  attribute* over a selection (e.g. logarithmic mass distribution via a
  range slider context action) (5.3)
- **Joints as selectable entities**: pickable, right-click opens their
  configuration (motors, limits — 4.5), movable in pause mode, never
  displaced by body resize (2.8, 4.1)
- Inspector re-architecture: context-menu-first, inspector as pop-out (2.8)
- Collision-layer set visualization UI (5.4)
- Re-diagnose "hinge behaves like weld" with joint selection + circle
  radius indicator + orbit view in hand (4.1)

## M14 — Grids & snapping (CAD pass)

- Major/minor grid lines; snap points provably on the grid at every
  adaptive zoom level (3.3, 2.5)
- Axis-lock basis follows the active grid system (2.5)
- Light collinear/centerline snapping while dragging ("lightweight
  assemblies") — configurable in the snapping menu (3.1)
- Alternate constructors: 3-point box, tangent circle (3.1)
- Curvilinear abstraction: tools operate in grid coordinates (polygon
  edges curve in polar grids) (3.3)
- Snap glyph stability, tangent glyph, snap-off-when-grid-hidden (3.4)

## M15 — Rendering & camera polish

- Emissive material option; ambient occlusion / contact shadows for the
  clay-matte look (9.3)
- Body borders: default dark-gray outline, per-body border color and
  transparency via context menu (1.5)
- Themed default palette; quantized color picker; random colors within a
  grouped selection (1.5)
- Camera settings section (zoom sensitivity etc.) (10.2)
- Sim-settings UI: scrub-drag values, gravity direction widget (8.2)

## M16 — Constraints II

- Weld rework: merge bodies into one (SDF `Union` — the tree makes this
  natural) or make-static, replacing the weld-as-joint model (4.2)
- Slider default limits option; sprite-based joint glyphs with outlines
  (4.7); motor state (direction/torque) visualization (4.1)
- Springs/dampers, cams, planar contact, magnetism (SDF force fields),
  breaking limits, backlash (12)
- Contact point & force debug overlays (2.6, 8.3)
- Engine tuning: timestep/substeps in Simulation settings, substep debug
  view (8.3)

## M17 — CSG modeling & pieces

- Boolean operations between bodies via context menu (join / subtract /
  intersect / xor) producing analytic trees (7.3)
- Piece velocity inheritance `v + ω × r` on severing cuts (7.5)
- Smooth-union (fillet) modeling tools (12)

## Backlog / later

- Curve pickers (lightroom-style), symbolic & equation input (12)
- Tracers, scripting, fluids (12)
- Investigate: load-time crash reported with a pre-M12 partial-cut save
  (11.1) — cuts no longer produce those trees, but saved ones must render;
  add a Csg tessellation robustness proptest when touching the mesher.
