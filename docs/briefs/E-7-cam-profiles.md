# Cam profiles from a curve
Phase E · roadmap § "Unified plan" · status: ready (after E-1)

Written by the planning thread, 2026-11-21. Closes M20's open "cams" with a shape, not a constraint. Docs only.

## Goal

"Make cam" on a selected body (and `(spawn-cam x y base lift-curve)`):
the response-curve editor shapes lift against angle (0..360°); the cam is a
closed path whose radius is `base + lift(θ)`. A follower on a slider,
pressed against it by a spring or gravity, rides the profile by ordinary
contact when the cam turns on a motorized hinge.

## Read first

- `docs/interaction-feel.md` — row **CAD** (numbers first: base radius, lift).
- `crates/gradiance-ui/src/curve.rs` — the curve widget; `gradiance-kernel` `Lut`.
- `docs/briefs/E-1-path-leaf.md`.

## Facts

- The curve editor's control points are sampled to a `Lut` at compile
  time; a cam needs the same sampling once, at edit rate, to anchors.
- Contact against a smooth path is as good as its flattening tolerance.

## Approach

- Feel: **CAD**. Pure `geometry::cam::cam_path(base, lut, samples) -> PathDef`
  (smooth anchors with handles from the derivative).
- Operation `body.make_cam`; the inspector shows the curve for a cam body
  (an authored `CamProfile` beside the shape, so it stays editable; editing
  it re-derives the path in the same command).

## Out of scope

A cam-follower constraint (contact only). Graph-driven cams (G-4).

## Done when

- Pure test: a constant lift gives a circle of `base + lift`.
- Behaviour test: a single-lobe cam at 1 rev/s lifts a follower by the
  lobe height ± 5 % once per revolution.
- Gates green.

## Outcome
