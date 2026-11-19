# Air drag and wind
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Global linear and quadratic air drag and a wind vector in `SimSettings`.
Quadratic drag uses the body's projected width across the relative air
velocity, so a plate falls slower face-on than edge-on.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `SimSettings` is scene content (`gradiance-domain/src/settings.rs`
  ≈ line 314); a new field is classified once in `scene_content!`.
- Projected width comes from the `polygonize` contour (the single
  discretization point), cached per shape change, not per frame.
- The force joins the `physics::fields` pass.

## Out of scope

Per-body drag coefficients (a later field on `BodyPhysics` if wanted).

## Done when

- Terminal velocity of a box matches sqrt(2mg / (rho Cd A)) within 2 %
  (behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
