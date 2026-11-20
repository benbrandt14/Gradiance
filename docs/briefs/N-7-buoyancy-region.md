# Buoyancy region (water)
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A body marked as fluid (a sensor with a fluid density and drag) pushes up
every dynamic body inside it by rho·g·(submerged area) at the submerged
centroid, and damps it. Boats float, heavy bodies sink.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `BodyPhysics.sensor` exists; collisions already ignore sensors.
- The submerged area is a polygon clip of two `polygonize` contours. The
  clip and the area/centroid maths go in `gradiance-geometry` (pure,
  tested). Density is areal (kg/m²) by the units decision, so the force
  is consistent in 2D.
- The force joins the `physics::fields` pass.

## Out of scope

Waves, surface tension, particle fluids.

## Done when

- A box of half the fluid's density floats half submerged (within 3 %,
  behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-n7`.

**Landed:**

- `gradiance-domain/src/fluid.rs` (new): the authored `Fluid { density:
  Density (areal kg/m², default 2), drag: f32 (1/s, default 2) }`, on any
  body. `BodyRecord::fluid` (serde default: old scenes load and behave the
  same, no format bump; the v8 golden sets one, the replay goldens gain
  `fluid: None`), `PropertyValue::Fluid` (one undo step), cut pieces
  inherit it, `BodySnapshot::fluid` (the old half of a script edit).
- `gradiance-geometry/src/submerge.rs` (new, pure, unit-tested): a
  `FluidVolume` (convex pieces as half-planes: one for a convex outline or
  a sea, the fill triangles otherwise), a `SolidOutline` (rings, outlines
  CCW and holes CW, decimated like the collider), and `clip_ring`
  (Sutherland–Hodgman), exact for any ring against a convex piece, so the
  submerged area and centroid are exact for the polygons. Allocation-free
  with a reused `ClipScratch`.
- `gradiance-physics/src/buoyancy.rs` (new): derived `FluidRegion` (built
  by `sync_fluid_regions` when a fluid is added or its shape changes,
  dropped when the fluid goes) and `SubmergeOutline` (built in
  `body_sync::sync_colliders` beside the collider and the air profile; that
  system now polygonizes a shape once for all three); the pure `Immersion`
  law and `apply_buoyancy`, one `FixedUpdate` system with the thrusters and
  air drag: `F = −ρAg − ρkA(v_c − v_fluid)` at the submerged centroid
  `c`, torque `−ρkA(I/m)ω`, drag capped so one step cannot reverse the
  motion.
- Command: `property::fluid_changes`, shared by the inspector and the new
  `body.set_fluid` operation `(set-fluid i density [drag])` (density ≤ 0
  turns it off); turning a fluid on also makes the body a sensor and, if
  dynamic, static, in the same undo step. `docs/ops.md` regenerated;
  `docs/scripting.md` notes the verb.
- UI: inspector **fluid (buoyancy)** toggle with `fluid density`
  (`precise_drag_unit`, Density) and `fluid drag (1/s)` (`precise_drag`).
  No snapshot covers the inspector; no snapshot changed.
- Render: a fluid body draws its fill blended at 45 % (`FLUID_OPACITY`),
  so floaters stay visible (checked on screen with `tools/screenshot`: a
  blue pool with three boxes half in it).
- Tests: `tests/it/buoyancy.rs` —
  `a_box_of_half_the_fluid_density_floats_half_submerged` (the "done when":
  measured fraction 0.49995, tolerance 3 %, upright and at rest after
  10 s), `a_light_plank_floats_at_its_density_ratio_in_a_sea` (half-plane
  fluid, ratio 1/4: measured 0.2497), `a_heavy_box_sinks` (reaches the
  terminal speed `(ρ_b/ρ − 1)g/k` within 5 %: 2.53 vs 2.5 m/s),
  `a_dropped_tilted_plank_settles_upright` (dropped 2 m at 0.4 rad: no
  runaway, upright, at rest, half submerged after 10 s),
  `a_fluid_is_authored_and_its_region_derived` (op, one undo step for
  fluid + sensor + role, saved, region not saved and follows the shape,
  density 0 turns it off), `a_plain_sensor_holds_no_fluid`. Unit tests in
  geometry (8: wedge, concave tank, holes, sea reject, scratch reuse),
  physics (4: law, drag, flow, cap) and command (2: the toggle batch). The
  persistence proptest carries fluids.

**Per-step cost:** with no fluid, one empty-query check. Otherwise
`O(F · B)` bounding-circle rejects (a few multiplies each) for `F` fluids
and `B` dynamic bodies, then per overlapping pair `O(P · V)` for the clip —
`P` the fluid's convex pieces (1 for a box tank or a sea; a concave fluid
pays its triangle count), `V` the body's ring vertices (4 for a box, 48 for
a circle) — plus one force and one torque. No allocation once the scratch
has grown, no contour.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`fluid-authoring`, `fluid-defaults`, `fluid-toggle`, `fluid-force-law`,
`fluid-step` (its own `FixedUpdate` system, not the `Update` fields pass the
brief names), `fluid-scope`, `fluid-region-pieces`, `fluid-look`.

**Not done / follow-ups:** a fluid acts across depth bands (see
`fluid-scope`); no surface drawing or ripples (waves out of scope); a
half-plane sea draws as the ground does, not translucent; a concave fluid
is triangulated without merging triangles into larger convex pieces (fine
for a box or a few-vertex pool, more clip work for a contoured CSG pool).
