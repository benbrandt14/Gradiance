# Air drag and wind
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Global linear and quadratic air drag and a wind vector in `SimSettings`.
Quadratic drag uses the body's projected width across the relative air
velocity, so a plate falls slower face-on than edge-on.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `SimSettings` is scene content (`gradiance-domain/src/settings.rs`
  ≈ line 314); a new field is classified once in `scene_content!`.
- Projected width comes from the `polygonize` contour (the single
  discretization point), cached per shape change, not per frame.
- The force joins the `physics::fields` pass.

## Out of scope

Per-body drag coefficients (a later field on `BodyPhysics` if wanted).

## Done when

- Terminal velocity of a box matches sqrt(2mg / (rho Cd A)) within 2 %
  (behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-n4`.

**Landed:**

- `gradiance-domain/src/settings.rs`: `SimSettings` gains `air_density`
  (`Density`, areal over the 1 m world depth, default 1.2 = sea-level air),
  `drag_coefficient` (Cd, default 0 = off), `linear_drag` (N·s/m², default
  0 = off) and `wind` (`Velocity2`, default 0). Scene content through the
  existing `sim` row of `scene_content!` (saved, one undo step per settled
  edit). Additive with serde defaults: old scenes load and behave the same,
  no format bump; the v8 golden gains the four fields (regenerated).
- `gradiance-geometry/src/profile.rs` (new): `WidthProfile`, a shape's
  projected width (the hull's extent across a flow direction) sampled at 64
  directions over a half turn; `width_across` is a wrap and a lerp.
- `gradiance-physics/src/air.rs` (new): the derived `AirProfile` component,
  `DragLaw` (pure: `F = -(c w + ½ρ Cd w |r|) r`, `r = v - wind`, `w` the
  width across `r` in the body frame; capped so one step cannot more than
  stop a body relative to the air) and `apply_air_drag`, one `FixedUpdate`
  system (with `apply_thrusters`, `sim_running`). `body_sync::sync_colliders`
  inserts the profile beside the collider on every shape change, from the
  same polygonized outlines (all components for CSG; zero for half-planes).
- UI: Settings > Simulation > **Air** section, five config rows
  (`labelled_drag`): drag, linear drag, air density, wind x, wind y. The
  `settings_grid` snapshot shows the new reflect rows (regenerated).
- Script: `sim-set`/`sim-get` paths `air_density`, `drag_coefficient`,
  `linear_drag`, `wind.x`, `wind.y` (in `SIM_SETTING_PATHS`); no new
  operation, `docs/ops.md` unchanged. `docs/scripting.md` notes them.
- Tests: `tests/it/air_drag.rs` —
  `a_falling_box_reaches_the_quadratic_terminal_velocity` (the "done when":
  a 2 x 0.5 m box at Cd 1, ρ 1.2 settles at `sqrt(2mg/(ρ Cd w))` within
  2 %, and stays there), `a_plate_falls_slower_face_on_than_edge_on`
  (ratio sqrt(10) within 5 %), `linear_drag_settles_at_mg_over_cw`,
  `the_wind_carries_a_body_through_drag_only`, `drag_is_off_by_default`,
  `the_profile_is_derived_and_follows_the_shape`,
  `a_drag_edit_is_one_undo_step`. Unit tests in geometry (profile) and
  physics (law, wind, cap, bad settings). The persistence proptest carries
  the new fields.

**Per-step cost:** with both drag terms 0 (the default) `apply_air_drag`
reads one resource and returns. Otherwise `O(B)` over bodies with a
profile: per dynamic body one `atan2`, one table lerp, a few multiplies
and one `apply_force`; no allocation, no contour. The profile build is
`O(64 · n)` per shape change, beside the collider build (a CSG tree is
contoured once more for it).

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`air-drag-law`, `air-drag-width`, `air-drag-step`, `air-drag-torque`,
`air-drag-surface`.

**Not done / follow-ups:** no aerodynamic torque or angular drag (a plate
keeps its attitude; flutter would need a centre-of-pressure torque).
Per-body drag coefficients (out of scope). The Air section was not checked
on screen beyond the reflect-grid snapshot. With the default body density
(1 kg/m²) air at 1.2 is as dense as the bodies, so Cd 1 makes things fall
slowly (about 4 m/s for a 1 m box); that is physics at this world's
density, not a tuning choice.
