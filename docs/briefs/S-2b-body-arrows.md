# Per-body velocity, momentum and force arrows
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("velocity / momentum / force arrows") · status: done

Split from S-2 by its worker (2026-11-22): S-2 landed the readouts and the
phase plot; the arrows touch the authored appearance, the inspector and the
renderer, and are a cycle of their own.

## Goal

A body can show its own velocity, momentum and net-force arrows while it
plays, each switched on in its Appearance section. The arrow scale is a
workstation setting.

## Read first

- `docs/briefs/S-2-readouts-and-xy-plots.md` (the parent brief and its Outcome).
- `crates/gradiance-domain/src/appearance.rs` — `Appearance` (authored, saved).
- `crates/gradiance-render/src/debug_viz.rs` — the global velocity overlay
  (`debug.velocities`), the drawing to reuse.
- `crates/gradiance-physics/src/queries.rs` — `LiveBody`: velocity, mass,
  contact force; S-2's `BodyAcceleration` gives the net force (`m·a`).

## Approach

- `Appearance.arrows: BodyArrows { velocity, momentum, force }` (bool flags,
  `serde(default)`), scene content: saved and undoable through the
  appearance edit the inspector already makes.
- One render system draws the arrows from `physics::queries` reads
  (velocity, `m·v`, `m·a`); allocation-free; scale and colours from a
  workstation setting and theme tokens (no colour literals outside the
  theme).
- Inspector: three checkboxes in Appearance.

## Out of scope

Field arrows (the `debug.fields` overlay covers them); per-contact force
arrows (`debug.contacts`).

## Done when

- A test: a body with `arrows.velocity` set draws one arrow per frame along
  its velocity (gizmo count or a pure helper), none when unset.
- The flags save and undo (undo matrix covers the appearance edit).
- A lavapipe screenshot on the PR. fmt, clippy, tests, doc gate green.

## Outcome

Done (2026-11-22).

- `Appearance.arrows: BodyArrows { velocity, momentum, force }` is authored
  scene content: saved (skipped in files when all off, so older files and
  goldens are unchanged) and undone through the appearance edit.
- `render::arrows::draw_body_arrows` is one allocation-free system. It
  draws from the centre of mass, reading velocity and mass through
  `physics::queries` and the force as `m·a` from S-2's derived
  `BodyAcceleration`. An arrow under 1 mm is not drawn. Kinds are lifted a
  hair apart in depth so a short momentum arrow along the velocity draws on
  top. The colours are theme tokens `ARROW_VELOCITY` / `ARROW_MOMENTUM` /
  `ARROW_FORCE`.
- **Scale** is workstation config `CameraSettings::arrow_scale`
  (0.25 m per unit, so 1 m/s, 1 kg·m/s or 1 N draws 0.25 m), sanitized
  ≥ 0, with a row in Settings > Camera. It lives with the camera feel
  because it is a view preference.
- **Surfaces.** An inspector "arrows" row (three checkboxes) in Appearance.
  The verb `(set-arrows i velocity momentum force)` (`body.set_arrows`,
  one `property-edit` step; the undo matrix covers it from its example).
  Group properties `arrow-velocity`, `arrow-momentum` and `arrow-force` in
  the body-property table, so `group-set`, filters and spreads reach them.
- Tests: `arrows::only_the_asked_arrows_are_drawn` (the pure selection)
  and `tests/it/body_arrows.rs` (one undo step, saved, undo clears, the
  group property reaches every member).
- Checked on lavapipe: a launched ball shows its velocity (yellow), its
  momentum (blue) and gravity's force (red). A box landing shows the impact
  as a long red force arrow for the step it lands in, which is real (its
  acceleration that step), and no arrows once it rests.
- **Learned:** a light body's momentum arrow is short at the default scale
  (a 0.28 kg ball at 2 m/s draws 14 cm). That is the units, not a bug; the
  scale setting adjusts it. Per-kind scales would be a follow-up if wanted.

