# Continuum & particle simulation — trade study

Status: **spike / decision doc** (2026-07-13, branch `claude/sim-spike`).
Scope: choose the substrate for Gradiance's bulk-matter simulation (sand,
fluids, snow, mud, smoke, debris) and the seam it plugs into. No solver is
committed by this document — it recommends a **staged** path and fixes the
architecture so the first slice doesn't paint later slices into a corner.

This pairs with the accompanying **particle spike** (`src/sim/`, `sim`
feature) which proves the seam end-to-end with an N-body particle system
(the `particular` crate) before any continuum solver is written.

## 1. Requirements (what a sandbox actually needs)

| Requirement | Consequence for the choice |
|---|---|
| **Multi-material** — sand, water, snow, mud, elastic goo, from one tool | favors a solver where material is a *constitutive-model switch*, not a different engine |
| **Two-way rigid coupling** — bodies float, sink, get buried, splash | the solver must read avian colliders and push impulses back through the sanctioned seam |
| **Interactive** — authoring at 60 fps, 10k–100k particles on 4 cores | CPU, SoA, allocation-free per-frame kernel; GPU is a later lever |
| **2.5D depth** — matter lives in a `DepthBand`, collides by depth overlap | the sim is fundamentally 2D fields/particles tagged with a depth band |
| **Fits the invariants** — derived, never authored | particles/grid are rebuilt state: never commands, never undo, never saved |
| **Fields-native** — gravity, magnets, and future fields already exist | the solver samples the *one* field cut-point, never re-implements forces |
| **Reversible spike** — must not bloat the default build | heavy deps live behind a feature until the crate split |

Non-goals for the first slices: destruction/fracture of authored rigid
bodies (that is a `ShapeDef` CSG operation, not a continuum solve),
production GPU MPM, and cloth/rods (1-D structural — a different substrate).

## 2. The candidate solvers

### A. SPH — Smoothed-Particle Hydrodynamics (WCSPH / PCISPH / DFSPH)

Pure particles, no grid; pressure from a kernel-summed density.

- **+** Simplest mental model; trivially meshless; great for splashy water;
  maps directly onto the particle spike's data layout.
- **+** Two-way rigid coupling is local (boundary particles / direct forcing).
- **−** Stiff pressure → small timesteps (WCSPH) or a global solve (DFSPH);
  neighbor search each step (hash grid) is the real cost.
- **−** Sand/snow/elastic need bolt-on models (granular SPH, peridynamics);
  **not** one unified solver — exactly the multi-material weakness.

### B. PIC / FLIP / APIC — grid-transfer fluids

Particles carry state, a background grid does the pressure projection;
FLIP/APIC transfer velocity back to reduce dissipation.

- **+** Incompressible fluids look excellent; the grid solve is cheap and stable.
- **+** APIC transfer is the same machinery MPM needs — a natural stepping stone.
- **−** Fluid-only. Sand/snow/elastic are not expressible without…

### C. MPM — Material Point Method (and MLS-MPM)

The generalization of APIC to arbitrary constitutive models: particles carry
deformation state, a grid does the momentum solve, particles and grid
exchange via APIC transfers each step. **MLS-MPM** (Hu et al. 2018) fuses the
transfer and force computation with a moving-least-squares stencil, cutting
the per-step cost to roughly PIC levels.

- **+** **One solver, every material.** Water, sand (Drucker–Prager
  plasticity), snow (Stomakhin), elastic (fixed-corotated), mud (viscoplastic)
  differ only in the stress update — precisely the multi-material requirement.
- **+** Two-way rigid coupling is natural: rigid bodies are velocity boundary
  conditions on the grid; the net grid impulse pushes back on the body.
- **+** Grid is regular → SoA, cache-friendly, embarrassingly parallel, a
  clean GPU port later. APIC is angular-momentum conserving (no PIC mushiness).
- **−** Most machinery to build: grid alloc/reset, P2G, grid update, G2P, a
  plasticity return-map per material. MLS-MPM tames the constant factor but
  not the part count.
- **−** Grid resolution bounds detail and cost; sparse grids (later) needed
  for large empty domains.

### D. Position-Based (PBD / PBF / XPBD)

Constraint projection instead of forces; PBF is the fluid variant.

- **+** Unconditionally stable, large timesteps, easy rigid coupling; great
  for real-time games.
- **−** Non-physical stiffness/parameters; multi-material is again bolt-on;
  overlaps philosophically with avian's own XPBD solver (avian2d *is* XPBD),
  so a second constraint solver is redundant substrate.

### Off-the-shelf crates

There is **no** production Rust MPM crate to depend on (as of the cutoff);
MPM will be hand-rolled. For particles/N-body there **is** `particular`
(N-body accelerations, `glam`-native, brute-force + Barnes–Hut) — ideal for
the **spike** and for genuine N-body toys (gravity wells, charged particles),
though not a continuum solver. SPH/PBF crates exist but are immature and would
be more wrapping than writing.

## 3. Integration axes (independent of solver choice)

- **Data layout.** Particles as **SoA** buffers in one derived resource
  (`Vec<Vec2>` position/velocity + material id), *not* one ECS entity per
  particle — 100k entities would swamp the scheduler and archetype moves.
  The grid (if any) is a flat `Vec` reset each step.
- **Timestep.** The sim advances on avian's **fixed clock** (`Time<Fixed>`,
  the same `timestep_hz` the rest of physics uses), sub-stepped internally if
  CFL demands it. Never on the render clock — determinism and coupling both
  need the fixed step.
- **Rigid coupling.** Read authored colliders through the existing
  `physics::queries` facade; sample forces through the **one** field
  cut-point `fields::Fields::accel_at`. Push reactions back as avian
  `ExternalImpulse`/`Forces` on the coupled bodies — the same seam
  `apply_field_forces` already uses. No new mutation path.
- **2.5D.** Particles carry a scalar depth (or inherit an emitter's
  `DepthBand`); coupling and inter-particle interaction gate on depth
  overlap, exactly like body collision. The solve stays 2D per depth slice.
- **CPU vs GPU.** CPU first (SoA + `rayon`-ready loops). The regular MLS-MPM
  grid is the cleanest possible GPU port when the particle budget forces it —
  designing SoA now keeps that door open.
- **Authored vs derived.** Authored: an **emitter/region** component
  (position, rate, material, initial velocity, depth) — small, reflect-
  registered, persisted, undoable via the normal command path. Derived:
  every particle and grid cell — rebuilt each step, **never** serialized,
  **never** in undo, **never** read by commands (CLAUDE.md invariant 5;
  `docs/scripting.md` "bulk/particle updates are derived").

## 4. Recommendation

**Target MLS-MPM as the continuum core, reached in stages, with an N-body
particle system shipping first as the seam's proof and as a feature in its
own right.**

Rationale: the sandbox's defining requirement is *many materials from one
tool*, and MPM is the only candidate that delivers that from a single solver
with a per-material stress update. Everything else (SPH, PBF) would relitigate
the multi-material problem per material. The cost — MPM is the most code — is
mitigated by staging and by the fact that APIC/MLS transfers are shared with
the fluid-only stepping stone, so no work is thrown away.

### Staged plan

1. **Particle substrate (this spike).** SoA particle buffer, authored
   emitter, Tier-B update system on the fixed clock, field-cut-point
   sampling, instanced rendering. Driver: `particular` N-body forces — real,
   useful, and it exercises every seam MPM will need. Behind the `sim`
   feature so the default build is untouched.
2. **APIC fluid.** Add the background grid + P2G/G2P transfers; water only
   (weakly-compressible or a grid pressure solve). Reuses the buffer,
   emitter, clock, coupling, and rendering from stage 1.
3. **MLS-MPM multi-material.** Generalize the grid force update to a
   constitutive-model switch: fluid → sand (Drucker–Prager) → snow → elastic.
   This is the payoff slice; it is *additive* over stage 2.
4. **Scale & polish.** `rayon` parallel P2G (atomic or colored scatter),
   sparse grid, optional GPU port; two-way coupling hardening.

### Crate split (the build-perf gate)

Keep stages 1–2 as a **feature-gated `src/sim/` module** (`sim` feature,
off by default) so the heavy deps and long compile stay out of the default
CI/test loop. Promote to a **`gradiance-sim` workspace crate** at the stage-2
→ stage-3 boundary, when the kernel stops churning and the compile cost is
paid every build — measured, per the build-perf decision gate (`cargo
build --timings`, adopt on >30% incremental improvement). The module is
written now with that promotion in mind: pure numeric core (`sim::kernel`,
no ECS) split from the one ECS-touching seam (`sim::bridge`), mirroring
`geometry/` and `script/`, so the split is mechanical.

### What this pins down for slice 1

- One authored `Emitter` component; one derived `Particles` SoA resource.
- One Tier-B system on `Time<Fixed>` that (a) samples `Fields::accel_at`,
  (b) runs the driver kernel (particular now, MPM later), (c) integrates,
  (d) couples to rigid bodies through `physics::queries` + impulses.
- Rendering reads the buffer and draws instances — no per-particle entity.
- Nothing in `src/sim/` is serialized or undoable; emitters are.
