# Sketch tool
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A freehand stroke becomes one body: the stroke simplified (RDP) into
segments, each a capsule, joined as one SDF union. `polygonize` makes the
collider and mesh, so the tool adds no geometry path. A closed stroke
fills as a polygon instead.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `ShapeDef` is an SDF tree with `Csg` nodes; `Capsule` is a leaf.
- RDP simplification is pure math: it goes in `gradiance-geometry` with
  tests.

## Out of scope

Pressure-sensitive width.

## Done when

- A stroke of 200 samples gives one body of fewer than 30 leaves; undo
  removes it in one step.
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
