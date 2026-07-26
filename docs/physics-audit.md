# Physics constants & avian-API audit (2026-07-24)

A sweep of every physics-affecting constant/default and avian call after the
world→metres (SI) flip, checking that defaults are well placed and the engine
APIs are used correctly. Units are SI throughout: metres, kilograms, seconds,
newtons; gravity `g = |(0, −10)|` m/s², collider density `1.0` kg/m².

## World / solver config — OK

| Setting | Value | Verdict |
|---|---|---|
| `PhysicsPlugins::with_length_unit` | `1.0` | **Correct.** avian scales its internal contact/sleep/tolerance lengths by this; it must approximate the typical dynamic-object size. Post-flip bodies are ~0.5–2 m, so `1.0` fits (pre-flip, pixel-scale bodies would have needed ~100). |
| `Gravity` / `SimSettings.gravity` | `(0, −10)` | Realistic SI; both the startup `GRAVITY` const and the settings default agree. |
| `SubstepCount` | `6` (clamped 1..64) | Reasonable for stiff joints/motors. |
| `timestep_hz` | `60` | Standard fixed step. |
| `LAYER_HEIGHT` | `0.1 m` | Depth-slab thickness; SI. |

## Body property defaults — OK

`friction 0.5`, `restitution 0.3`, `density 1.0` kg/m², `gravity_scale 1.0`.
All dimensionless/SI and scale-independent. `restitution 0.3` is mildly bouncy
but intentional.

## Joint motors

avian's motor `max_force`/`max_torque` is a **ceiling**: each substep the
corrective impulse is clamped to `max_force · dt²`. Under the
acceleration-based model (`MotorModel::AccelerationBased { stiffness: 0,
damping }`, the correct choice for *velocity* control — `SpringDamper` always
carries a position term and is for position control) the impulse a body needs
to reach its target velocity scales with its **inertia** (hinge) / **mass**
(slider). Consequences and fixes:

- **Ceiling was fixed `1.0e7`** → wrong at both ends: heavy bodies' need
  exceeded it (weak/"negligible"); light bodies' need was a tiny fraction, so
  the impulse was effectively unbounded and, being above the engagement-impulse
  threshold `~ damping · target / dt · I`, spiked the rigid point constraint on
  the first substep — the pivot drifted ("too compliant") and light bodies were
  flung. **Fixed:** the *default* ceiling now scales with the connected body
  (`MOTOR_TORQUE_PER_INERTIA`, `MOTOR_FORCE_PER_MASS`), sitting above the
  gravity load but below the spike threshold. Stays user-editable.
- **`damping = 30`** (velocity-tracking gain, 1/s): firm but stable per substep
  (`damping · dt ≈ 0.08`). Left as a documented tuning knob — the ceiling was
  the dominant instability lever.
- **Oscillate reversal** measured the relative angle as `rot_b − rot_a`,
  **omitting the rest basis** (`rest_rot_a − rest_rot_b`) that avian's
  `with_local_basis2` / `with_angle_limits` are relative to — so the reversal
  never lined up with the real limit and the motor just drove into the stop.
  **Fixed** to `wrap((rot_b − rot_a) + basis)`, which is `0` at the creation
  pose and matches avian's constraint frame.
- Target velocity is authored in SI (rad/s, m/s) but **shown in rpm** for
  hinges (the familiar motor unit), converting on display/commit.

## Struts (spring-damper) — fixed SI staleness

- `SPRING_STIFFNESS_PER_MASS = 100`: **correct in SI** — `k = m·g/sag` gives
  `k ≈ 100·m` for a ~0.1 m sag. Only the comment was stale (`px²`, `|g|≈1000`).
- `mass_proxy` floored AABB area at `1.0` — a pixel-era floor that over-stiffened
  any sub-metre body. **Removed** (area is real m² now); `MIN_SPRING_STIFFNESS`
  still guards the low end.
- `DEFAULT_SPRING_STIFFNESS = 0.1` (inspector reset fallback) was a mechanical
  `÷PIXELS_PER_METER²` rescale — ~1000× too soft (a reset strut drooped ~100 m).
  **Fixed to `100`** to match the tool's typical mass-based value.

## Tool thresholds — SI (fixed earlier this branch)

`box MIN_SIDE 0.01`, `circle MIN_RADIUS 0.005`, `ground FLAT_THRESHOLD 0.05`,
`select MOVE_EPSILON 0.005`, `connector AXIS_THRESHOLD 0.05`, `cut MIN_STROKE
0.01`, `drag MOVE_EPSILON 0.005`, `polygon CLOSE_RADIUS 0.08`, `strut
MIN_STRUT_LENGTH 0.05`, oscillate `ANGLE_BUFFER 0.05 rad` / `TRANSLATION_BUFFER
0.02 m`. All metre-scale (~0.5–8 px on screen); screen-space picks
(`*_PX * cam_scale`) are unaffected by the flip and stay in pixels.

## avian API usage — OK

- `RevoluteJoint`/`PrismaticJoint` point compliance defaults to `0` (rigid) —
  correct; pivot drift was the motor ceiling, not joint compliance.
- World pins spawn a `RigidBody::Static` anchor with no collider — correct.
- Derived state (`Collider`, `Mass`, avian joint entities) is rebuilt by
  `Changed<>` sync and never serialized — matches invariant #5.

## Validated headless (avian steps in the `tests/it/` harness)

avian runs without a window, so the motor fixes are covered in CI, not just
"tested in-app":

1. **Pivot stability** (`motorized_hinge_holds_its_pivot`) — a strong motor
   spins a large arm without shoving the pivot off its pin, and actually spins
   it (guards against both the old drift *and* the "negligible torque" end).
2. **Oscillation with a rest basis** (`world_pinned_tilted_motor_oscillates`) —
   a world-pin hinge authored at a tilt reverses at *both* bounds, the case the
   zero-basis body-body test missed. Confirms the basis-inclusive reversal
   frame (and its sign).
3. The auto ceiling is derived from real `ComputedAngularInertia`/`ComputedMass`
   in `joint_sync`, so it applies to authored, programmatic, and loaded motors
   alike.

## Still feel-dependent (nudge in-app)

- **Motor feel** — `damping = 30` (gain) and the ceiling coefficients
  (`MOTOR_TORQUE_PER_INERTIA` / `MOTOR_FORCE_PER_MASS`) are the tuning knobs;
  the stable-band structure is fixed, the absolute feel is taste.
- **Contact "launch"** — the bounded ceiling removes the impulse spike a
  dropped body used to get; a rotating surface still imparts its tangential
  surface velocity (expected). Re-check the magnitude in-app.
