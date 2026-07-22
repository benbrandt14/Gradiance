# Engineering units — design decision & phased plan

Status: **accepted (directional)**. The four shaping decisions below were
ruled by the owner; this revision records them and the evidence behind the
representation choice. Physics-behaviour is deliberately held constant this
pass (2D density), so the only behaviour-adjacent step is a units-only save
migration.

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
- **The roadmap needs it.** Sensors ("proximity and force"), plotters (axes
  need units), parameter-linking ("restitution based on proximity … math
  in menus"), and material-property editing all consume physical
  quantities. Built once, they share a substrate.

## Current unit inventory

| Quantity | Today | SI target |
|---|---|---|
| position, dimensions, distance, depth | px (world units) | metre (m) |
| rotation | rad | rad (angle, dimension-flagged) |
| linear velocity | px/s | m/s |
| angular velocity | rad/s | rad/s |
| acceleration, gravity | px/s² | m/s² |
| mass | derived: density × area | kilogram (kg) |
| density (`ColliderDensity`) | mass / px² | **kg/m² (areal — decision 3)** |
| force (`ContactForce`) | impulse/dt, px-mass units | newton (N) |
| torque | implicit | N·m |
| friction, restitution, gravity_scale, `speed` | dimensionless | dimensionless |
| timestep (`timestep_hz`) | Hz | Hz (1/s) |
| spring stiffness / damping (M20 struts) | avian compliance/damping | N/m, N·s/m |

## The four rulings

### 1. Representation → **reflection-native newtypes; external crate stays behind the API**

Owner intent: *use external crates as building blocks, but nothing may
fight the reflection features.* The evidence says those two goals split
cleanly along one line — **what gets stored/reflected vs. what does
conversion math.**

Stored/queried/catalog quantities are our own `#[repr(transparent)]`
newtypes over `f32`, holding a **base-SI** value, in a new pure crate
`gradiance-units` (no bevy; sits beside `gradiance-kernel`). Examples:
`Length`, `Area`, `Angle`, `Mass`, `Density`, `Velocity`, `Acceleration`,
`Force`, `Torque`, `Stiffness`, `Damping`, `Frequency`, plus `Vec2`-shaped
`Displacement`/`Velocity2`/`Force2`. Arithmetic relations we actually use
get explicit operator impls (`Force = Mass * Acceleration`,
`Area = Length * Length`) — dimension-checked on the common paths without
nightly `generic_const_exprs`.

**Why not store an external crate's quantity directly — the reflection
evidence.** The scripting bridge (`gradiance-script/reflect_bridge.rs`,
the "reads are total" path) reads values by `downcast`/`try_apply` against
concrete `f32`/`f64`/`u32`/`i32`/`bool`, and `reflect_to_steel` recurses
only into `ReflectRef::Struct`. Two consequences, read straight from that
code:

- A `uom`-style `Quantity<…>` is not `Reflect` at all → can't be an
  authored field, and would degrade to `Void` for scripts/plotters. This is
  exactly the reflection fight to avoid, so the crate is **excluded from
  reflected/persisted types.**
- Even our own newtype is a **tuple struct**, which today falls through
  `reflect_to_steel` to `Void`, and `read_path("mass")` misses unless you
  write `"mass.0"`. So newtypes need a **small, one-file bridge change**:
  teach `scalar_to_steel`/`read_path`/`write_path`/`reflect_to_steel` to
  unwrap a single-field tuple struct to its inner scalar. After that,
  newtypes read as plain numbers — full strong typing, no permanent fight.
  This change is part of the pass (P0).

An external units crate is **welcomed as an internal building block** for
the parse/format/dimensional-conversion layer — user unit entry ("3 cm",
"5 lb/ft") and future dimensional-heavy derivations — used *behind* the
`gradiance-units` API and never appearing in a reflected or serialized
field. It is adopted when the in-scope DSL-units work (P6) needs
multi-unit parsing, not pulled in speculatively; base-SI-only conversions
until then are trivial and hand-written. (Candidate crates evaluated at
that point; the API is designed so the choice is swappable.)

### 2. Source of truth → **SI-authored; persistence revved (format v6)**

Owner intent: *take advantage of the strong typing, which means revving
persistence to accommodate.* So the typed quantities are the **stored**
representation — records serialize each newtype as its base-SI scalar — and
the scene format revs to **v6**. Pixels become a render/pick concern,
reached only through the one conversion seam.

- `core::world::WorldScale` (`PIXELS_PER_METER`) is the **single** px↔SI
  seam: `to_world(Length) -> f32 px` / `from_world(px) -> Length`.
  `PIXELS_PER_METER` is removed from physics/field math and made reachable
  only through `core::world` (enforced like `CommandStack` confinement).
- **v5→v6 migration** is units-only: stored lengths ×`1/PIXELS_PER_METER`,
  velocities likewise, gravity px/s²→m/s², density px→areal-SI
  (× `PIXELS_PER_METER²`, see decision 3). No topology or component change,
  so it is low-risk and fully round-trip testable.

The rollout is still incremental (each phase green), but unlike the earlier
draft there is **no interim pixel-stored phase** — the persistence rev is
committed, because the strong typing is the point.

### 3. Density → **2D areal (kg/m²) now; built to jump to 3D later**

Owner ruling: *2D density only; be broadly prepared for a future 3D jump,
but we're not there yet.* Density stays **areal, kg/m²**, and the depth
band stays inert for mass — **no physics-behaviour change** this pass.

The single future-proofing requirement: mass is computed in **one place**,
`units::mass_of(density: Density, area: Area) -> Mass`, and nothing else
multiplies density by geometry. A future 3D jump is then a localized edit —
change that one function to `density_3d × area × thickness(DepthBand)` and
swap the `Density` dimension kg/m²→kg/m³ — without touching call sites.
`Density`'s dimension is defined so that swap is a one-line change in
`dimension.rs`.

### 4. Cross-cutting bundle → **all four; coordinate frames on its own path**

Owner ruling: *do all four, but coordinate frames is important enough for a
dedicated path — keep the door open.* So this pass includes:

- **PhysicalQuantity catalog** — one registry: each measurable quantity =
  `{ name, dimension (→ display unit), reader over physics::queries }`. The
  extensibility keystone: adding a quantity is one row, like the
  `command_intents!` table and the scripting operation registry. Units, P2
  sensors, and plotter/tracer axes all bind to it; `SignalSource`'s variants
  become catalog lookups.
- **UI SI display + input** — inspector/settings render unit-labelled SI and
  parse SI input, via `units::format`/`parse`. A workstation unit-system
  display toggle (SI ⇄ alias) is a display resource, not authored state.
- **Units in the expression DSL** — dimensional quantities in the
  parameter-linking / functional-notation menus ("restitution ∝ proximity").
  This is where an external parse crate may be adopted (behind the units
  API). Binds to the catalog + typed quantities.
- **Coordinate frames** — a **dedicated separate path**, not built here, but
  the door is held open: the quantity catalog and typed lengths are
  **frame-agnostic** (values carry dimension, not a frame), and the
  `WorldScale` seam is where a frame transform later composes. Nothing in
  this pass forecloses frames; a `docs/frames-decision.md` follows.

## Architecture

```text
gradiance-units  (NEW, pure — beside gradiance-kernel at the bottom)
  ├─ quantity.rs   typed newtypes (base-SI f32) + the arithmetic we use
  ├─ dimension.rs  Dimension enum → canonical unit + formatter/parser
  ├─ mass.rs       mass_of(Density, Area) — the ONE density×geometry seam
  └─ catalog.rs    PhysicalQuantity registry (name·dimension·reader-key)

gradiance-core
  └─ world.rs      WorldScale (PIXELS_PER_METER) — the ONE px↔SI seam.

gradiance-script
  └─ reflect_bridge.rs  extended to unwrap single-field newtypes → scalar
                        (P0; makes typed quantities read-total-native).

domain / scene     authored components carry typed quantities;
                   records serialize base-SI scalars; format v6.
physics            touches avian world-units ONLY through core::world;
                   queries.rs returns typed quantities; catalog readers here.
signal             SignalSource → catalog quantity; values carry dimension.
render             SI→px for draw at the seam (never mid-shader).
ui                 units::format/parse; unit-system display toggle.
```

`tests/boundaries.rs` gains `gradiance-units` as a new bottom DAG node (no
`gradiance-*` deps; every layer may depend on it, like `kernel`), and a
check that `PIXELS_PER_METER` is named only in `core::world`.

## Extensibility contract (the point of the pass)

- **New measurable quantity** → one `PhysicalQuantity` catalog row. Sensors,
  plotters, scripts pick it up with no further code.
- **New authored physical field** → give it a typed quantity; formatting,
  parsing, SI persistence, and script reflection come for free.
- **New unit dimension** → one `Dimension` variant + canonical unit +
  formatter.
- **2D→3D density** → edit `units::mass_of` + one `dimension.rs` line.
- **Coordinate frames** → compose a transform at the `WorldScale` seam; the
  catalog is already frame-agnostic.
- **Never** reintroduce a bare `f32` for a dimensional value in authored or
  UI-facing code — a boundary test scans `domain` records for non-allowlisted
  `f32` fields, mirroring the serde-confinement test.

## Phasing (each phase leaves fmt+clippy+test green)

0. **P0 — reflection spike + bridge unwrap.** A throwaway `Mass(f32)`
   newtype: prove it reflects, and extend `reflect_bridge.rs` so
   `read_path`/`write_path`/`reflect_to_steel`/`scalar_to_steel` unwrap a
   single-field tuple struct to its inner scalar. Lock it with a test
   (`(mass . 2.5)`, not `Void`). De-risks the owner's #1 concern first.
1. **P1 — `gradiance-units` crate + `core::world` seam.** Newtypes,
   dimensions, formatter/parser, `mass_of`, `WorldScale`. Confine
   `PIXELS_PER_METER`; delete it from field/force math. No behaviour change.
   DAG test updated.
2. **P2 — retype geometry/physics/domain.** Thread typed quantities through
   the pure layers and the physics boundary; `physics::queries` returns
   typed quantities. Save format still v5 here (values still pixel on disk
   until P5) — or fold P5 in if the retype naturally reaches records.
3. **P3 — `PhysicalQuantity` catalog + signal/plotter binding.** Catalog in
   `gradiance-units`, readers in `physics`; `SignalSource` and plot/probe
   panels bind to it; axes get unit labels.
4. **P4 — UI SI display + input.** Inspector/settings format & parse SI;
   unit-system display toggle.
5. **P5 — scene format v6 (SI-stored).** Records serialize base-SI; units-only
   v5→v6 migration; round-trip + value-preservation tests. (2D density; the
   `mass_of` seam is the 3D-ready cut-point.)
6. **P6 — units in the expression DSL.** Dimensional quantities in
   parameter-linking menus; adopt an external parse crate behind the units
   API if multi-unit entry is wanted.
- **(dedicated later) coordinate frames** — `docs/frames-decision.md`;
  door held open by the frame-agnostic catalog + `WorldScale` seam.

## Verification & risk

- The existing suite pins physics behaviour; because density stays 2D,
  **no physics assertion should move** in P0–P6. P5's migration gets a
  dedicated v5→v6 round-trip + value-equivalence test.
- Coverage/coupling CI (already landed) tracks the new crate automatically.
- Lowest-risk ordering: P0 settles the reflection question with a test
  before any breadth; the only format break (P5) is a pure units conversion,
  not a topology or behaviour change.
