# Particle fluid (spike and decision)
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("particle water") · status: done

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

Decide, with a measured spike, how particle water runs in Gradiance: a
derived particle population stepped by one Tier-B system, pushing bodies
and pushed by them. The spike shows a column of water that falls, spreads
on a floor and lifts a light box. The output is a decision note plus the
spike code behind a feature flag, not a finished feature.

## Read first

- `docs/script-spike-findings.md` spike 2 — the kernel speed at particle scale.
- `crates/gradiance-physics/src/fields.rs` — the per-step force seam bodies already use.
- `crates/gradiance-physics/src/buoyancy.rs` — the region fluid (N-7, Q-5).
- CLAUDE.md "Two tiers, one language" — bulk updates are derived, never commands.
- Roadmap § Nonlinear dynamics — the 2026-11-19 non-recommendation, and why.

## Facts

- Particles are derived: never saved per particle, never in undo. What is
  authored is the *emitter* (S-11): a region and a particle spacing.
- Body coupling is the risk: avian has no particle API. Candidates: (a)
  particles as a separate solver that reads body colliders through
  `SpatialQuery` and applies equal and opposite impulses through
  `ExternalForce`; (b) particles as small avian bodies. *Unverified*
  which holds at 2 000 particles at 60 Hz in a debug build.

## Approach

- Position-based fluid (PBF) or SPH with a uniform hash grid, SoA buffers,
  allocation-free per step, in `gradiance-physics` (pure step maths in
  `gradiance-kernel` or a new pure module).
- Render as instanced discs first; a metaball surface is a later brief.
- Write `docs/particle-fluid-decision.md`: chosen method, coupling, budget,
  what S-11 builds on.

## Out of scope

Liquify and the water tools (S-11). Surface rendering. Saving particles.

## Done when

- Spike scene: 2 000 particles, step time measured and recorded.
- Behaviour test: a box of half the water density floats (±10 %) on
  settled particle water.
- Decision note committed; fmt, clippy, tests, doc gate green.

## Outcome

Done (2026-11-22). The decision note is `docs/particle-fluid-decision.md`.

- **Method:** double-density relaxation (Clavet 2005, no springs), four
  substeps per physics step, a uniform grid, persistent buffers. Pure step
  maths in `physics::particles::solver` (no ECS); the one system is
  `physics::particles::step_particle_fluid` in `FixedUpdate`.
- **Coupling:** candidate (a). Particles read bodies as SDFs and pay
  dynamic bodies equal and opposite impulses through `Forces`. A wall
  table (`WallTable`) counts the solid as fluid for a particle by a body;
  without it a half-density box sat 57 % submerged.
- **Gate:** a runtime gate (no particles: the step returns), not a Cargo
  feature, so CI runs the test. Decisions `particle-fluid-gate` and
  `particle-fluid-coupling` in `docs/review/archive.ron`.
- **Render:** overlay discs (`render::particles_viz`).
- **Measured:** 2 000 particles step in 7.2 to 8.1 ms per physics step in
  the dev profile; settled density 1 018 kg/m²; a box of half that density
  floats 50.6 % submerged.
- **Tests:** `tests/it/particle_water.rs` (the spike scene and the float
  test), `particles::solver` unit test (a column settles, the floor
  carries the weight within 15 %).

