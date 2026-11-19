# Laser node
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A placeable node that casts a ray along its axis, reflects off bodies up
to N times, and draws the beam. It publishes a bus value: 1 while the
beam reaches a named target body, else 0, and the hit distance. Light
gates and reflection puzzles then work through N-1.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- avian `SpatialQuery` ray casts are available in `physics`; the read
  goes into `physics::queries` so scripts get it too.
- Nodes: see N-3. Beam drawing uses the overlay gizmo group.

## Out of scope

Refraction and colour mixing.

## Done when

- A beam at 45 degrees onto a mirror wall hits a target body placed by
  geometry (behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
