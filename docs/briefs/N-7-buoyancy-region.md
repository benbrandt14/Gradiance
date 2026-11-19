# Buoyancy region (water)
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

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

(empty)
