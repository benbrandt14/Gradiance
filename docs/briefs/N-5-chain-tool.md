# Chain and rope tool
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Drag a path; release makes N capsule links joined by hinges, one undo
step. Options: link length, width, end pins (to the bodies under the ends,
or to the world).

## Facts (planning thread, 2026-11-19, on `92ab302`)

- Spawn and joint intents exist (`spawn-body`, `spawn-joint`); one
  gesture commits one command (invariant 2), so the command batches the
  links and joints.
- `ShapeDef::Capsule` exists (`geometry::polygonize` ≈ line 41).
- The rod tool (`tools::rod_tool`) is the closest gesture to copy.

## Out of scope

A slack rope as one distance-limit joint (can follow as a mode).

## Done when

- A chain pinned at both ends hangs as a catenary (sag within 5 % of
  the analytic value, behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
