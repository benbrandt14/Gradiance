# Gear outline and gear tool
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("gear tool", "transform into gear") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

A gear tool (and `(spawn-gear x y r teeth)`, and "Make gear" on a circle)
makes a toothed body. Two gears on hinges mesh through ordinary contact and
turn at the ratio of their tooth counts.

## Read first

- `crates/gradiance-geometry/src/shape.rs:70` — `ShapeDef` leaves.
- `docs/sdf-geometry-decision.md` — what a new leaf costs.
- `crates/gradiance-geometry/src/sketch.rs` — a shape built from
  parameters at commit time (the cheaper pattern).
- Roadmap § Nonlinear dynamics: "gears (no avian gear constraint known)".

## Facts

- The 2026-11-19 non-recommendation concerned a gear *constraint*. The
  reference app's gears are outlines that mesh by contact. This brief is
  geometry only; no new constraint.
- Contact meshing can chatter at high speed. *Unverified* how much on
  avian 0.8; the worker measures before adding anything.

## Approach

- Pure `geometry::gear::gear_outline(radius, teeth, depth) -> Vec<Vec2>`
  (trapezoid or simplified involute teeth), authored as a `Polygon` leaf at
  commit. No new SDF leaf unless the polygon path fails.
- `body.spawn_gear` operation, Gear palette entry (no key), Adjust Last on
  teeth and radius. "Make gear" replaces a circle's shape in one step.
- Colliders: the tooth count is capped so the convex decomposition stays
  within the existing piece budget.

## Out of scope

A gear-ratio joint. Racks, bevel gears.

## Done when

- Pure test: outline has `2 × teeth` tooth flanks and the pitch radius ± 1 %.
- Behaviour test: a 12-tooth gear motor-driven at 1 rad/s turns a
  24-tooth gear at 0.5 rad/s ± 5 % over 5 s.
- fmt, clippy, tests, doc gate green.

## Outcome
