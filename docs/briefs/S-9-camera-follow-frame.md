# Camera follow, frame selection and camera verbs
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("follow", "zoom to fit", "camera zoom / pan from script") · status: ready

Written by the parity audit thread, 2026-11-20 (`docs/parity-audit.md`). Docs only.

## Goal

The camera can follow a body while the scene plays, frame the selection or
the whole scene, and be set by script: `(camera-follow i)`,
`(camera-frame targets)`, `(camera-look x y zoom)`.

## Read first

- `crates/gradiance-interaction/src/camera.rs:51` — `CameraRig`, the single
  derived camera source.
- `crates/gradiance-interaction/src/ops.rs` — `camera.get`/`camera.set`.
- `crates/gradiance-ui/src/menu.rs` — View menu rows.

## Facts

- `Home` already glides back to 2D through `CameraRig::homing`. Reuse the
  glide for framing.
- Camera state is workstation config: never undo, never scene content.

## Approach

- `CameraRig.follow: Option<StableId>`; one system moves the focus to the
  body each frame (read through `physics::queries`).
- Frame: bounds of the targets (`world_bounds` in `context_menu.rs:856`
  moves down to a lower crate if two callers need it).
- Operations `camera.follow`, `camera.frame`, `camera.look`; View menu rows
  and the `.` key for frame selection (check `KEY_OPS` for conflicts).

## Out of scope

Camera keyframes, multiple viewports.

## Done when

- Tests: follow keeps the body within 1 px of centre over 2 s of fall;
  frame puts every target inside the viewport.
- fmt, clippy, tests, doc gate green; `docs/ops.md` regenerated.

## Outcome
