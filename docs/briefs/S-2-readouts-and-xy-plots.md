# Body readouts, XY plots and per-body vector arrows
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("information", "plot y vs x", "velocity / momentum / force arrows") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

The quantity catalog gains area, moment of inertia, gravitational potential
energy and acceleration. The Plot panel can plot one signal against another
(phase plot). A body can show its own velocity, momentum and net-force
arrows while it plays.

## Read first

- `crates/gradiance-domain/src/quantity.rs` — the catalog; `:43` inertia
  is already read by the solver, `:153` kinetic energy.
- `crates/gradiance-domain/src/signal.rs:25` — `SignalSource`.
- `crates/gradiance-ui/src/plot.rs:274` — the time-only x axis.
- `crates/gradiance-render/src/debug_viz.rs` — the global velocity overlay.

## Facts

- `debug.velocities` draws every body's velocity. Nothing draws momentum
  or net force per body (checked 2026-11-20).
- Acceleration needs two samples: a derived per-body `PrevVelocity` in
  `FixedPostUpdate`, or a finite difference on the signal history.
  *Unverified* which one is cheaper; the worker measures.

## Approach

- Catalog rows: `area`, `inertia`, `potential` (m·g·h against the scene
  gravity, h from the world origin), `accel`. Each row also becomes a
  `SignalSource`, so plots and N-1 sinks read it.
- Plot: an optional x signal per plot. Absent = time (today's behaviour).
- Arrows: a per-body `Appearance` flag set (velocity, momentum, force),
  scene content, edited in the Appearance section. One render system draws
  them from `physics::queries`. Scale is a workstation setting.

## Out of scope

Spring and field potential energy. Integral-on-hover.

## Done when

- Unit tests per new catalog row (a 2 kg box at h = 3 m reads 58.8 J under
  9.8 m/s²).
- A test plots `PosX` against `Height` of a projectile and gets a parabola
  (fit residual below 1 %).
- fmt, clippy, tests, doc gate green.

## Outcome
