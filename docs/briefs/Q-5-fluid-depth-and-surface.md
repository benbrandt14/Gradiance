# Fluid by depth band, and a water surface
Phase Q (follow-ups) · N-7 Outcome "a fluid acts across depth bands", "no
surface drawing", "a half-plane sea draws as the ground does, not
translucent" · status: done

Written by the planning thread, 2026-11-20, on `523ed82`.

## Goal

1. **A fluid acts only on bodies in its depth.** A body whose depth band
   shares no collision layer with the fluid's passes in front of or behind
   the pool without buoyancy or drag, as it passes other bodies (collision
   layer ≡ visual depth). This reverses the N-7 decision `fluid-scope`;
   the planning thread recorded the new choice as `fluid-depth-scope`.
2. **A water surface is drawn.** The upward-facing edges of a fluid's
   outline (against gravity) draw as one line, a lighter tint of the
   fluid's fill, 2 screen pixels wide, in front of the fill. A sea draws
   its surface line across the view. No waves or ripples. Decision
   `fluid-surface-look`; the owner reviews it on the PR's screenshot.
3. **A sea is translucent** like a pool: a half-plane fluid draws its
   ground plane at `FLUID_OPACITY`, so a floater's lower half shows.

Scenes from before this brief that have a pool and floaters in one band
(the default band for every new body) behave the same.

## Read first

1. `docs/review/archive.ron` entries `fluid-scope`, `fluid-look`,
   `fluid-depth-scope`, `fluid-surface-look`.
2. `crates/gradiance-physics/src/buoyancy.rs:180-260`
   (`apply_buoyancy`).
3. `crates/gradiance-render/src/material_sync.rs:20-60` and
   `crates/gradiance-render/src/plane.rs` (the ground material).
4. `docs/ui-design.md` § colours and tokens (the tint comes from the look,
   not a literal).

## Facts (planning thread, 2026-11-20, verified on `523ed82`)

- `apply_buoyancy` (`buoyancy.rs:187`) queries fluids as `(&Fluid,
  &FluidRegion, &Position, &Rotation, Option<&LinearVelocity>)` and bodies
  as `(Forces, &RigidBody, &ComputedMass, &ComputedAngularInertia,
  &ComputedCenterOfMass, &SubmergeOutline)` with `Without<Fluid>`; neither
  query reads a depth band or collision layers.
- Collision layers are derived from the band:
  `CollisionLayers::from_bits(bits, bits)` in `body_sync`
  (`crates/gradiance-physics/src/body_sync.rs:155`), bits from
  `DepthBand::bits` (`crates/gradiance-domain/src/depth.rs:95`). The laser
  already masks by the riding body's layers (decision
  `laser-reflects-everything`).
- A fluid region is a sensor (turning a fluid on makes the body a sensor
  in the same step), so its `CollisionLayers` exist and are not used for
  contacts.
- `FLUID_OPACITY = 0.45` (`material_sync.rs:25`); the blend is applied in
  `body_material` (`:48-57`); half-planes skip the body material and draw
  through the ground material (`material_sync.rs:154-157`;
  `plane.rs:173`, `:201`).
- `FluidVolume::half_plane()` is the sea `y ≤ 0` in the fluid's frame
  (`crates/gradiance-geometry/src/submerge.rs:154`).
- Body outlines draw as gizmo line strips in
  `crates/gradiance-render/src/decorations.rs:22` (`draw_body_borders`),
  in front of the cap at `band.z_front() + 0.005`.

## Approach

- Physics: add `&CollisionLayers` to both queries; skip a pair whose
  memberships do not intersect (one AND per pair, before the bounding
  reject). No allocation added.
- Geometry (pure, unit-tested): `surface_edges(outline, up) -> segments`,
  the outline edges whose outward normal is within 60 degrees of `up`, in
  the body frame. Built where `FluidRegion` is built (shape change), kept
  on the derived region; never per frame.
- Render: one system draws each fluid's surface segments in world space
  (`up` = −gravity direction from `SimSettings`), colour from the look's
  fill lightened (one helper next to `load_colour`), width from the overlay
  gizmo config. A sea draws one segment across the view's x range.
- Ground material: an alpha for a fluid half-plane (the plane shader
  already has a colour; add the blend mode for that case only).

## Tests

- `tests/it/buoyancy.rs`: a box half the fluid's density in a band that
  does not overlap the pool's falls through it at free-fall acceleration;
  moved into the pool's band it floats half submerged (the N-7 case).
- Geometry unit tests: a box pool gives its top edge; a tilted box gives
  the edges facing up; a U-shaped tank gives its two rims; a sea gives
  the surface line.
- The N-7 tests still pass unchanged.
- fmt, clippy, doc, `--test boundaries`, `-p gradiance-geometry`,
  `-p gradiance-physics`, `-p gradiance-render`, the touched `--test it`
  modules.
- Lavapipe screenshots on the PR (not committed): a pool with floaters, a
  sea with a plank, a body in another band in front of the pool. These are
  the owner's review of `fluid-surface-look`.

## Out of scope

- Waves, ripples, splashes, flow visuals.
- Merging a concave fluid's triangles into larger convex pieces (N-7 note;
  a cost item, not a defect).
- Any change to the force law.

## Outcome

Done (2026-11-21); the owner's screenshot review of `fluid-surface-look`
is still to do (no lavapipe run in this session).

- **Depth:** `apply_buoyancy` reads both sides' `CollisionLayers` and skips
  a pair whose memberships share no bit (a missing layer reads as all
  bits). A sea is a half-plane, whose layers are all bits, so it holds every
  band. Test `a_body_in_another_band_passes_the_pool_by` (free fall in
  another band; floats half submerged in the pool's band).
- **Surface:** `geometry::submerge::surface_edges(contours, up)` is pure and
  allocation-free (outward normal within 60° of up, `SURFACE_COS`). It reads
  the cached `ShapeContours` per frame (`O(V)`, no contour), so the brief's
  "build where FluidRegion is built" was not needed: a fluid that turns
  keeps the right surface for free. `render::decorations::draw_fluid_surfaces`
  draws it in the fill lightened 45 % toward white (`surface_colour`),
  opaque, on the overlay gizmos. A sea draws ±5 km along its surface.
- Unit tests: box top, gravity sideways, a tilted box, U tank, air pocket.
  Note: a U-shaped *fluid body* also has its channel floor facing up, so it
  gives three edges (two rims and the floor), not two.
- **Translucent sea:** `rebuild_ground` blends a fluid half-plane at
  `FLUID_OPACITY` (`AlphaMode::Blend`). `plane.wesl` now keeps the
  material's alpha and fades it to opaque in the horizon fog (opaque grounds
  are unchanged). A ground that stops being a fluid turns opaque again
  (`RemovedComponents<Fluid>`). Shader change not yet checked on lavapipe:
  shaders fail at run time.
