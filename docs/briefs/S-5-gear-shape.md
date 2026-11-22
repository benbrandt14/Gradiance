# Gear outline and gear tool
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("gear tool", "transform into gear") · status: done

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

Done (2026-11-22).

- **Measured first:** straight trapezoid teeth (the brief's first option)
  jammed a 12:24 pair after about one tooth. Two causes, both measured in
  `tests/it/gears.rs`:
  1. The polygon collider came from avian's approximate convex
     decomposition, which made the 12- and 24-tooth gears 5 and 2 hulls
     and filled the tooth gaps (collider area 1.25 vs 1.13 m² outline).
  2. Straight flanks are not conjugate: with the gaps open, the pair
     turned at a ratio near 0.48 and then locked at a tooth tip.
- **Outline:** `geometry::gear::gear_outline` generates the teeth as a hob
  does: the standard basic rack (20° pressure angle, addendum 1 module,
  dedendum 1.25 modules, tooth 0.44 of the pitch for backlash) rolls along the
  pitch circle and removes what it sweeps. Every gear of one module is
  then conjugate to every other, and small gears get their undercut. One
  pitch is generated (`SAMPLES_PER_TOOTH` = 20) and repeated. Teeth 6 to
  48. A `Polygon` leaf; no new SDF leaf.
- **Collider:** a `Polygon` leaf now collides as its **exact** convex
  partition (`geometry::partition::convex_partition`: the fill
  triangulation merged by Hertel-Mehlhorn), one hull a piece. The
  approximate decomposition stays the fallback and stays for CSG
  contours. This changes every polygon leaf (polygon tool, closed
  sketches, cut pieces): their notches now collide as drawn. The gears
  are 48 and 96 pieces.
- **Surface:** `(spawn-gear x y r teeth)` (`body.spawn_gear`),
  `(make-gear i [teeth])` (`body.make_gear`, a property edit of the
  shape; non-circles are skipped; teeth absent: the default module), the
  Gear tool (Create group, `gear.png`, no key; press the centre, drag the
  pitch radius; commits through `body.spawn_gear`, so Adjust Last edits
  radius and teeth), and "Make gear" in the context menu. The tool and
  "Make gear" use `DEFAULT_MODULE` = 0.1 m, so any two of their gears mesh.
- `OpEntry::selection_params` now leaves optional fields out, so an
  operation whose other fields are all targets is clickable (F3, menus).
- Tests: outline flanks and pitch radius ± 1 %, tooth fraction, clamping
  (pure); exact partition covers an L, a ring and a gear with convex
  pieces (pure); the 12-tooth gear driven at 1 rad/s turns the 24-tooth
  gear at −0.499 rad/s over 5 s (± 5 %); `make-gear` and its undo.
- Decisions in `archive.ron`: `gear-teeth-generated`,
  `polygon-collider-exact`.

