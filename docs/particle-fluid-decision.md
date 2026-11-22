# Particle fluid — decision (brief S-10)

Status: decided by the S-10 spike, 2026-11-22. S-11 builds on it.

## Decision

1. **Method:** double-density relaxation (Clavet, Beaudoin and Poulin,
   SCA 2005) without the springs. It is position based, so it is stable at
   the physics step with four substeps, and it gives a surface without a
   surface term. A uniform grid (a counting sort per substep) finds the
   neighbours. Buffers persist between steps: a step allocates nothing for
   the particles once the population stops growing.
2. **Body coupling: a separate solver (candidate a), not avian bodies.**
   The particles read the bodies as signed distance fields
   (`geometry::sdf`, the same field the shapes are made of) and pay each
   dynamic body the equal and opposite impulse through avian's `Forces`
   (one-shot, like the field, thruster and buoyancy forces). Candidate (b),
   2 000 small avian bodies, was not built: it puts 2 000 rigid bodies and
   their contact pairs into the solver that the scene's own bodies need,
   and still has no pressure, so a pool would be a heap of balls.
3. **Wall pressure:** a particle by a body counts the solid as more fluid
   on its lattice (`WallTable`, a sampled half-plane sum). Without it a
   box of half the water density sat 57 % submerged (measured); with it,
   and the wall push split half and half as a fluid pair is, it sits 50.6 %
   submerged.
4. **Derived, gated at run time.** The population lives in the resource
   `physics::particles::ParticleFluid`: never saved, never in undo, never
   in a command. The spike has no authored way in; only code fills it.
   The brief asked for "a feature flag": a Cargo feature would keep the
   behaviour test out of CI (CI builds the default features), so the gate
   is the empty population instead (the step returns at once). Decision
   `particle-fluid-gate` in `docs/review/archive.ron`.
5. **Render:** one overlay disc per particle (`render::particles_viz`).
   A surface (metaballs) is a later brief.

## Measured

| What | Value | Where |
|---|---|---|
| Step time, 2 000 particles, 4 substeps, dev profile (our crates `opt-level = 1`) | 7.2 to 8.1 ms per physics step | `tests/it/particle_water.rs` (printed) |
| Settled density, 2 000 particles in a 2 m tank, spacing 0.04 m | 1 018 kg/m² (water at rest: 1 000) | same test |
| Box of half that density, 0.4 m | 50.6 % submerged (target 50 % ± 10 %) | same test |
| Floor reaction, 0.64 m pool | within 15 % of the weight | `particles::solver` unit test |
| Shallow pool area | about 11 % above the lattice area | same unit test |

Thus 2 000 particles take about half of a 60 Hz frame in a debug build.
*Not measured:* a release build (the container's disk cannot hold one
next to the debug build). The solver is not parallel; a parallel density
pass is the first speed-up if S-11 needs more particles.

## Constants (`FluidParams::water`)

Spacing 0.04 m; radius 3 spacings; stiffness 1 200 x spacing; near
stiffness 1 500 x spacing; viscosity 40 /s; particle mass 1 000 x
spacing² kg; contact radius half a spacing; friction 0.1. A scan picked
them: with a radius of 2 spacings, or a viscosity under 10 /s, a pool did
not settle in 6 s (mean speed above 0.1 m/s).

## What S-11 builds on

- `ParticleFluid::particles` (`Particles::push`, `fill_box`, `clear`) is
  the store an emitter fills when a run opens. The authored record is the
  emitter (outline and spacing), saved with the scene.
- Undo of a run must clear the population, as `ClearRemovals` (S-7) does
  for removed bodies.
- Known limits: the obstacle pass tests every particle against every body
  box (fine for tens of bodies); a fast thin body can tunnel through the
  water (no continuous test); particles do not sleep.
