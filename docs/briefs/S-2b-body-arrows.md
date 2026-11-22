# Per-body velocity, momentum and force arrows
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("velocity / momentum / force arrows") · status: ready

Split from S-2 by its worker (2026-11-22): S-2 landed the readouts and the
phase plot; the arrows touch the authored appearance, the inspector and the
renderer, and are a cycle of their own.

## Goal

A body can show its own velocity, momentum and net-force arrows while it
plays, each switched on in its Appearance section. The arrow scale is a
workstation setting.

## Read first

- `docs/briefs/S-2-readouts-and-xy-plots.md` (the parent brief and its Outcome).
- `crates/gradiance-domain/src/appearance.rs` — `Appearance` (authored, saved).
- `crates/gradiance-render/src/debug_viz.rs` — the global velocity overlay
  (`debug.velocities`), the drawing to reuse.
- `crates/gradiance-physics/src/queries.rs` — `LiveBody`: velocity, mass,
  contact force; S-2's `BodyAcceleration` gives the net force (`m·a`).

## Approach

- `Appearance.arrows: BodyArrows { velocity, momentum, force }` (bool flags,
  `serde(default)`), scene content: saved and undoable through the
  appearance edit the inspector already makes.
- One render system draws the arrows from `physics::queries` reads
  (velocity, `m·v`, `m·a`); allocation-free; scale and colours from a
  workstation setting and theme tokens (no colour literals outside the
  theme).
- Inspector: three checkboxes in Appearance.

## Out of scope

Field arrows (the `debug.fields` overlay covers them); per-contact force
arrows (`debug.contacts`).

## Done when

- A test: a body with `arrows.velocity` set draws one arrow per frame along
  its velocity (gizmo count or a pure helper), none when unset.
- The flags save and undo (undo matrix covers the appearance edit).
- A lavapipe screenshot on the PR. fmt, clippy, tests, doc gate green.

## Outcome
