# Engineering units — design decision & phased plan

Status: **accepted (directional)**. The four shaping decisions below were
ruled by the owner; this revision records them and the evidence behind the
representation choice. Physics-behaviour is deliberately held constant this
pass (2D density), so the only behaviour-adjacent step is the units rebase
of the save format (v6, with older files rejected — decision 2).

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
newtypes over `f32`, holding a **base-SI** value, in a new crate
`gradiance-units`. It is a bottom node with no `gradiance-*` deps, but —
unlike `gradiance-kernel` — carries a **minimal `bevy` surface** so the
quantities can `#[derive(Reflect)]` and live in authored components (the
same rationale as `gradiance-geometry`'s shape tree); `kernel` stays the
only fully-pure crate. Examples:
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
- **v6 is a hard break: v5 (and older) files are rejected**, not migrated
  (`from_ron` returns `PersistError::Version` for anything but 6). This
  follows the project's persistence-debt stance — the same pass that landed
  the flip also *removed* the surviving v4→v5 layer migration: pre-release,
  there are no persisted scenes worth preserving, and a units-only migration
  would still have to enumerate every length-typed field (poses, shape
  dims, joint anchors, rest lengths, depth bands) at the RON level, which is
  exactly the fiddly, bug-prone work the rev exists to retire. A real
  migration path can be reintroduced once the format stabilizes for release;
  until then, revving the version is the whole contract.

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
  become catalog lookups. **Readers take a *set* of `StableId`s** (0/1/2/N),
  not a fixed arity — so single-body (`Speed`), pair (`Distance`), and future
  **grouped-node aggregates** (average/min/max over a selection — a horizon
  item) are all the same shape. Building this set-capable now avoids a
  single-body-only rework later.
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
gradiance-units  (NEW — bottom node, no gradiance deps, minimal bevy)
  ├─ quantity.rs   typed newtypes (base-SI f32/Vec2) + the arithmetic we use
  ├─ dimension.rs  Dimension enum → canonical unit + formatter/parser
  ├─ mass.rs       mass_of(Density, Area) — the ONE density×geometry seam
  ├─ world.rs      PIXELS_PER_METER + SI⇄px conversions (the one scale seam)
  └─ catalog.rs    PhysicalQuantity registry (name·dimension·reader) — P3

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

## Forward compatibility — horizon items this pass must not wall off

Three larger directions are on the roadmap horizon (unscheduled,
`docs/roadmap.md` § Horizon). This pass is planned so each lands as an
extension, not a rewrite:

- **Full 3D / multiple simulation planes.** The `units::mass_of(Density,
  Area)` seam is the density-dimension cut-point (2D→3D is one function + one
  `dimension.rs` line), and `core::world::WorldScale` is where per-plane
  transforms compose. Typed quantities are already dimension-only, not
  plane-bound, so a second plane adds a frame at the seam, not new scalar
  types.
- **Grouped node behaviors.** Handled by the set-valued catalog readers
  above; an aggregate is a reader over a `SelectionGroup`'s ids, its result
  carrying the member quantity's dimension.
- **CAD kernel front-end.** Dimensioned constraints are unit-bearing, so
  typed `Length`/`Angle` and the DSL parse layer (P6, external crate behind
  the API) are direct enablers; a CAD front end *produces* `ShapeDef` geometry
  above `gradiance-geometry` and shares the coordinate-frames path.

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
2. **P2·flip — atomic SI value + format rebase (landed).** Rather than
   thread newtypes first and keep pixel values on disk, the world was
   rebased to SI in one atomic numeric conversion: constants, physics, and
   authored records now carry base-SI values, the render camera absorbs the
   ×`PIXELS_PER_METER` render factor (orthographic scale `1/PPM`), and the
   scene format revs to **v6 with v5 rejected** (decision 2). No newtype
   threading yet — signatures stay `f32`-SI; the newtypes remain available
   for the retype below. Physics behaviour is preserved up to the unit
   relabel (world ÷100 × camera ×100 = identical pixels).
3. **P2·types — retype geometry/physics/domain** (in progress, by accretion).
   Thread the `gradiance-units` newtypes through the pure layers and the
   physics boundary so `physics::queries` returns typed quantities. Pure
   refactor over the already-SI values from the flip — no value or format
   change. *Landed:* the `physics::queries` read facade returns typed
   quantities (`velocity_of → (Velocity2, AngularVelocity)`, `mass_of → Mass`,
   `net_contact_impulse → Impulse2`, `ContactSample::normal_impulse →
   Impulse`); the probe/plot UI reads unit labels off the quantity types
   (`Velocity::UNIT`, …) instead of hard-coded strings — which is how the
   post-flip "px/s" label drift was caught and fixed. The `ui → units` DAG
   edge was added for it (reviewed in `tests/boundaries.rs`).
4. **P3 — `PhysicalQuantity` catalog + signal/plotter binding.** Catalog in
   `gradiance-units`, readers in `physics`; `SignalSource` and plot/probe
   panels bind to it; axes get unit labels.
5. **P4 — UI SI display + input.** Inspector/settings format & parse SI;
   unit-system display toggle.
6. **P6 — units in the expression DSL.** Dimensional quantities in
   parameter-linking menus; adopt an external parse crate behind the units
   API if multi-unit entry is wanted.
- **(dedicated later) coordinate frames** — `docs/frames-decision.md`;
  door held open by the frame-agnostic catalog + `WorldScale` seam.

## Verification & risk

- The existing suite pins physics behaviour; because density stays 2D and
  the flip is a pure unit relabel, **no physics *behaviour* moves** — the
  test values that changed are the authored-magnitude literals and golden
  snapshots, rescaled ÷`PIXELS_PER_METER` alongside the source, plus the
  format break covered by a v5-rejection test (`pre_si_files_are_rejected`)
  and the replay golden round-trips.
- Coverage/coupling CI (already landed) tracks the new crate automatically.
- Lowest-risk ordering: P0 settles the reflection question with a test
  before any breadth; the one format break (the P2·flip → v6) is a pure
  units rebase, not a topology or behaviour change.
