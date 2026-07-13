# Fields: architecture

Status: **living document** (2026-07-12). Fields are deliberately built as a
cross-cutting substrate — particle simulation, visualization, the node
editor, user interactions, and the P3 symbolic layer all consume them — so
the shape of this seam matters more than any single field feature.

## The one sampling cut-point

`physics::fields::Fields::accel_at(point, exclude) -> Vec2` is the **only**
way anything reads the field. Today's consumers:

| Consumer | Use |
|---|---|
| `apply_field_forces` | mass-scaled solver forces on every dynamic body |
| the vector-plot overlay (`DebugSettings::show_fields`) | arrows on a screen grid — *what you see is what acts* |
| `SetOrbitRequest` (Algodoo "set in orbit") | `v = √(a·r)` about the dominant source |

Planned consumers plug into the same call: particle kernels sample it per
particle (Tier-B, allocation-free — `Fields` is a plain query wrapper, cheap
to reborrow), a node-editor **sensor** node is `accel_at` at a point, a
scripted probe is the same read through the registry, and plotters can
record it as a named signal. **A new field kind lands everywhere at once**
because nothing consumes sources directly.

## Authored model (Algodoo-shaped)

`FieldSource { strength, falloff }` on a body — strength is *signed
repulsion* (negative attracts), falloff is exactly `Linear | Quadratic`.
The field acts on **every** dynamic body (not only other sources), scaled
by target mass, so field acceleration is mass-independent and orbits work.

## SDF by default, not exclusively

A source's field is shaped by its SDF: magnitude decays over *surface*
distance, direction follows the SDF gradient — a plank repels away from its
faces. This is a property of today's one source kind, not of the seam:
future kinds (point charges, uniform wind volumes, scripted symbolic fields
from P3's `(grad …)`) implement the same "contribution at a point" contract
inside `Fields` and every consumer inherits them.

## Composition rules

- **Superposition.** Multiple fields sum (`accel_at` = Σ contributions).
  Field-on-field interaction beyond superposition (e.g. media/shielding) is
  explicitly out of scope until a concrete feature needs it.
- **Time variation** enters by *driving the authored knobs*, not by making
  the sampler stateful: `FieldSource::strength` is reflect-registered, so a
  P2 signal driver (or a plain scripted edit / keyframe) varies it through
  the existing seams and the sampler stays a pure function of world state.
- **Constraints win.** Field forces apply as one-shot avian `Forces`
  (cleared per step), like the grab spring and rotate torque — a field can
  never punch a body through a joint or a rotation lock.

## Classification (per the state table)

`FieldSource` is authored (save file, undoable via `PropertyValue::Field`,
serde-defaulted so pre-field scenes load). Everything derived from it —
forces, arrows, orbit velocities — is recomputed per frame and never
persisted.
