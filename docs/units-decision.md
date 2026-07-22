# Engineering units — design decision & phased plan

Status: **proposed** (planning artifact for the units pass). The four
shaping decisions below were chosen as recommended defaults in the absence
of a ruling; each is marked **(revisable)** and can be flipped without
rewriting the whole plan — the phasing isolates the risky ones.

## Why now

The tool computes real physics but speaks no engineering units. Every
quantity is a bare `f32`/`Vec2` whose unit lives only in a doc comment:
`SignalSource::Speed` is "px/s", `SimSettings::gravity` is "px/s²",
`DepthBand` is "world units into the screen." Consequences:

- **No safety.** Nothing stops a px/s value flowing into a m/s slot; the
  compiler can't help and reviewers can't see it.
- **The pixel↔SI factor leaks into physics.** avian is configured once with
  `.with_length_unit(PIXELS_PER_METER)` (`= 100`, so 100 px = 1 m, gravity
  `−1000` px/s²), but `PIXELS_PER_METER` also appears inside force/field
  math (`REFERENCE_FIELD_MASS = PIXELS_PER_METER²`, falloff divides by it).
  The scale is not confined to a seam.
- **Depth is physically inert.** Bodies carry a real thickness
  (`DepthBand{near,far}`) but `ColliderDensity` is treated as 2D areal, so
  a thicker body does not weigh more.
- **The roadmap needs it.** Sensors ("proximity and force"), plotters
  (axes need units), parameter-linking ("restitution based on proximity …
  math/functional notation in menus"), and material-property editing all
  consume physical quantities. Built ad-hoc, each re-invents unit handling;
  built once, they share a substrate.

## Current unit inventory (what exists, implicitly)

| Quantity | Today | SI target |
|---|---|---|
| position, dimensions, distance, depth | px (world units) | metre (m) |
| rotation | rad | rad (angle, dimension-flagged) |
| linear velocity | px/s | m/s |
| angular velocity | rad/s | rad/s |
| acceleration, gravity | px/s² | m/s² |
| mass | kg·(px-based) via density×area | kilogram (kg) |
| density (`ColliderDensity`) | mass / px² | kg/m³ (see decision 3) |
| force (`ContactForce`) | impulse/dt, px-mass units | newton (N) |
| torque | — (implicit) | N·m |
| friction, restitution, gravity_scale, `speed` | dimensionless | dimensionless |
| timestep (`timestep_hz`) | Hz | Hz (1/s) |
| spring stiffness / damping (M20 struts) | avian compliance/damping | N/m, N·s/m |

## The four shaping decisions

### 1. Representation → **hand-rolled typed quantities** (revisable)

A curated set of `#[repr(transparent)]` newtypes over `f32`, each storing a
**base SI value**, in a new pure crate `gradiance-units` (no bevy; sits
beside `gradiance-kernel` at the bottom of the graph). Examples: `Length`,
`Area`, `Volume`, `Angle`, `Mass`, `Density`, `Velocity`, `AngularVelocity`,
`Acceleration`, `Force`, `Torque`, `Stiffness`, `Damping`, `Frequency`,
plus `Vec2`-shaped `Displacement`/`Velocity2`/`Accel2`/`Force2`.

Arithmetic relations we actually use get explicit operator impls
(`Force = Mass * Acceleration`, `Area = Length * Length`,
`Mass = Density * Volume`), so the common paths are dimension-checked
without needing nightly `generic_const_exprs`.

- **Rejected: `uom`.** Its generic `Quantity<Dimension, Units, V>` is
  powerful but fights the three seams this codebase is built on —
  `bevy_reflect` derives (authored components must reflect), serde/RON
  persistence, and the scripting reflection bridge (spike 1). The
  integration tax outweighs the dimensional-analysis win for our finite
  quantity set.
- **Rejected: runtime-metadata-only.** Keeps bare `f32`; no compile-time
  safety — the exact failure mode we're removing.

Each newtype derives `Clone, Copy, PartialEq, Serialize, Deserialize,
Reflect` (opaque where needed) so it drops into authored components and
records unchanged.

### 2. Source of truth → **SI-authored, reached in two phases** (revisable)

End-state: **save files store base SI** (m, kg, N); pixels are a
render/pick concern only. This is the principled target — "authored state
*is* the save file," so the save file should be in engineering units, and
the DSL/scripts then see SI natively.

But flipping storage is a scene-format break, so we **stage** it to keep
each step green:

- **Phase A — typed, still pixel-stored.** Introduce `gradiance-units` and
  retype the code paths; the stored/authored numbers stay world-units, with
  a single conversion seam (`world::WorldScale`) at the avian boundary and
  at UI/script/IO edges. Zero format change; fully reversible.
- **Phase B — flip storage to SI.** Scene format **v6**: records store SI;
  a v5→v6 migration multiplies stored lengths by `1/PIXELS_PER_METER`, etc.
  The conversion seam moves to *only* the avian boundary (render reads SI
  and scales for draw). This is the one migration-bearing step and is
  isolated so it can be scheduled/deferred independently.

This ordering means the risky part (a) is opt-in and (b) lands after the
typed layer has proven itself against the test suite.

### 3. 2.5D density → **true 3D density, mass-preserving migration** (revisable)

Density becomes **kg/m³** and `mass = density × area × thickness`, where
`thickness = (far − near)` from the `DepthBand`. Depth stops being inert —
a deeper body weighs more, which is what "engineering units" should mean
for a 2.5D tool and unlocks realistic buoyancy/pressure later.

Behaviour-change guard: the v5→v6 migration is **mass-preserving** —
each body's new `density_3d` is back-computed so its effective mass equals
what the old areal density produced, so existing scenes feel identical
until a user deliberately edits depth or density. New-body defaults are
retuned to sensible real materials (≈ water, 1000 kg/m³, as the `1.0`
analogue).

- **Rejected: keep 2D areal (kg/m²).** Simpler and no behaviour change, but
  wastes the depth band and leaves mass disconnected from the 2.5D model
  the whole tool is built around.

### 4. Cross-cutting bundle (revisable)

Ride-along items, chosen by shared-substrate synergy:

- **✅ Core — `PhysicalQuantity` catalog.** One registry: each measurable
  quantity = `{ name, dimension (→ display unit), reader over
  physics::queries }`. This *is* the extensibility mechanism — adding a
  quantity is one row, mirroring the `command_intents!` table and the
  scripting operation registry. Units, the P2 sensor/actuator work, and
  plotter/tracer axes all bind to it, so building it here prevents three
  future re-inventions. `SignalSource`'s variants become catalog lookups.
- **✅ Core — UI SI display + input.** Inspector/settings render
  unit-labelled SI (`1.20 m`, `2.5 kg`, `9.81 m/s²`) and parse SI input,
  via `units::format`/`parse` helpers. A workstation-level unit-system
  toggle (SI ⇄ a display alias, e.g. cm/g) is a display resource, not
  authored state.
- **◻ Stretch — units in the expression DSL.** Dimensional quantities in
  the parameter-linking / functional-notation menus ("restitution ∝
  proximity"). Depends on the DSL work; designed-for now (the catalog +
  typed quantities are what it would bind to), built later.
- **◻ Separate pass — arbitrary coordinate frames.** Length-unit-aware
  plot/measurement frames. Related but sizeable; noted as a downstream
  consumer, not in this pass.

## Architecture

```text
gradiance-units  (NEW, pure — beside gradiance-kernel at the bottom)
  ├─ quantity.rs   typed newtypes + the arithmetic relations we use
  ├─ dimension.rs  Dimension enum → canonical unit + formatter/parser
  └─ catalog.rs    PhysicalQuantity registry (name·dimension·reader-key)

gradiance-core
  └─ world.rs      WorldScale (PIXELS_PER_METER) — the ONE px↔SI seam;
                   to_world(Length)->f32 px / from_world(px)->Length.
                   Removes PIXELS_PER_METER from physics/field math.

domain / scene     authored components carry typed quantities;
                   records serialize base SI (Phase B).
physics            reads/writes avian in world-units *only* through
                   world::WorldScale; queries.rs returns typed quantities;
                   catalog readers live here.
signal             SignalSource → catalog quantity; values carry dimension.
render             converts SI→px for draw at the seam (never mid-shader).
ui                 units::format/parse; unit-system display toggle.
```

The crate DAG test (`tests/boundaries.rs`) gains `gradiance-units` as a new
bottom node with no `gradiance-*` deps; every layer may depend on it (like
`kernel`). `PIXELS_PER_METER` becomes `pub(crate)`-reachable only through
`core::world`, enforced the same way `CommandStack` confinement is.

## Extensibility contract (the point of the pass)

- **New measurable quantity** → one `PhysicalQuantity` catalog row (name +
  dimension + reader). Sensors, plotters, and scripts pick it up with no
  further code — the same "one row" ergonomics as adding a command intent.
- **New authored physical field** → give it a typed quantity; it formats,
  parses, persists (SI), and reflects for scripting for free.
- **New unit dimension** → one `Dimension` variant + its canonical unit and
  formatter; the newtype that needs it references the variant.
- **Never** reintroduce a bare `f32` for a dimensional value in authored or
  UI-facing code — a clippy-style boundary check can scan for `f32` fields
  in `domain` records outside an allowlist, mirroring the serde-confinement
  test.

## Phasing (each phase leaves fmt+clippy+test green)

1. **P1 — `gradiance-units` crate + `core::world` seam.** Newtypes,
   dimensions, formatter/parser, `WorldScale`. Confine `PIXELS_PER_METER`
   to the seam; delete its appearances in field/force math (replace with
   typed conversions). No behaviour change. DAG test updated.
2. **P2 — retype geometry/physics/domain (Phase A, pixel-stored).** Thread
   typed quantities through the pure layers and the physics boundary;
   `physics::queries` returns typed quantities. Save format unchanged.
3. **P3 — `PhysicalQuantity` catalog + signal/plotter binding.** Catalog in
   `gradiance-units`, readers in `physics`; `SignalSource` and the plot/probe
   panels bind to it; axes get unit labels.
4. **P4 — UI SI display + input.** Inspector/settings format & parse SI;
   unit-system display toggle.
5. **P5 — scene format v6 (Phase B) + true-3D density.** Flip records to SI;
   mass-preserving v5→v6 migration; density becomes kg/m³. The single
   migration-bearing, behaviour-adjacent step — schedulable independently of
   P1–P4.
6. **(later) DSL units; coordinate frames** — designed-for, out of this pass.

## Verification & risk

- The existing suite pins physics behaviour; P1–P4 must not move any
  assertion. P5's migration gets a dedicated round-trip + mass-preservation
  test (old v5 scene → v6 → identical simulated masses).
- Coverage/coupling CI (just landed) tracks the new crate automatically.
- Biggest risk is P5 (format break + physics feel). It is deliberately last
  and self-contained, so P1–P4 deliver typed-unit safety and the
  sensor/plotter/UI wins even if P5 is deferred.
