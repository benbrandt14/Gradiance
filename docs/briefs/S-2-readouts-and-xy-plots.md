# Body readouts, XY plots and per-body vector arrows
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("information", "plot y vs x", "velocity / momentum / force arrows") · status: done (arrows split to S-2b)

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

Done (2026-11-22) except the arrows, which are split into
`docs/briefs/S-2b-body-arrows.md` (an authored appearance field, an
inspector row and a render system: a cycle of their own).

- **Catalog rows** `area`, `inertia`, `potential-energy` and `acceleration`
  (`domain::quantity`), each also a `SignalSource` variant (`Area`,
  `Inertia`, `PotentialEnergy`, `Acceleration`; bus tags `area`, `inertia`,
  `potential`, `accel`), so plots, sinks, `(quantity …)` and the node
  graph's body ports read them. `BodyRead` gained `area`, `gravity` and
  `acceleration` (default `None`); `BodySample` carries them.
- **New dimension** `Dimension::MomentOfInertia` (kg·m²): inertia had none.
- **Potential energy** is `−m·(g·p)` against the body's own gravity (scene
  gravity × its gravity scale), zero at the world origin
  (`quantity::potential_energy`; 2 kg 3 m up under 9.8 m/s² = 58.8 J,
  unit test).
- **Acceleration**: a derived `physics::acceleration::BodyAcceleration`,
  the velocity change over the last step, written by one allocation-free
  `FixedPostUpdate` system after avian's step, only while the sim runs.
  Chosen over a finite difference on the signal history because the
  history only exists for plotted signals; this costs one `Vec2` subtract
  per body per step.
- **Area** is mass over density, so it is the solver's area (the same
  polygonization the collider uses).
- **Phase plot**: the Plot panel's header has an `x:` choice, time (as
  before) or any series. Against a series, every visible series is drawn
  against it, paired by equal frame stamps (`plot::phase_points`, pure; a
  sample with no partner is skipped, never interpolated).
- Tests: catalog unit rows (`every_reader_reads_a_known_state`), the
  58.8 J case, `quantities::area_inertia_potential_and_acceleration_read_a_live_body`
  (area 2, inertia `m(w²+h²)/12`, free-fall acceleration = g, potential
  `m·g·h`), `plot::a_phase_plot_pairs_samples_by_their_stamps`, and
  `phase_plot::a_projectile_plots_as_a_parabola` (launched with S-1's
  `InitialMotion`; least-squares residual under 1 % of the rise).
- Checked on lavapipe: a ball launched with `set-velocity`, its `pos-x`
  and `height` bound to the plot; `x: px` draws `py` as the parabola with
  the x axis labelled `px [m]`. Two polish fixes from that look: the
  Overlay/Stacked choice is disabled in phase mode (it only applies to the
  time axis), and the bus clock `t` is left out of the x choices (it is the
  same axis as "time"). The `plot_pane` UI snapshot was regenerated for
  the new `x:` dropdown (the only change in the image).

