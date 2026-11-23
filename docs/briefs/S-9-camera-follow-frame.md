# Camera follow, frame selection and camera verbs
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("follow", "zoom to fit", "camera zoom / pan from script") · status: done

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

Done (2026-11-22).

- **Rig** (`interaction::camera`): `CameraRig.follow: Option<StableId>`
  and `CameraRig.view: Option<ViewTarget>` (a focus and a zoom in world
  metres per pixel). `CameraRequest` (`Follow`, `Frame`, `Look`) is the
  one way in; `apply_camera_requests` answers it, `follow_and_glide` moves
  the focus onto the followed body and glides toward a view. The glide
  uses the rig's `GLIDE_RATE` (the rate `Home` uses); the zoom glides in
  log space. A pan by hand stops a follow and a glide. Camera state stays
  editor state: no command, no undo step, not saved.
- **Pose read:** the follow reads the body's `Transform` (avian writes it
  back after each step). `physics::queries` has no pose accessor, and a
  pose is not a physics read, so none was added.
- **Frame:** `frame_view` fits a world box into the viewport with 10 %
  free on each side (a box under 1 m frames as 1 m). The bounds come from
  `geometry::sdf::world_aabb`, which moved down from the context menu so
  both callers share it.
- **Operations:** `camera.follow` (`(camera-follow i)`, -1 stops),
  `camera.frame` (`(camera-frame targets)`, an empty list frames every
  body but the grounds), `camera.look` (`(camera-look x y zoom)`, zoom in
  screen pixels per metre, 100 is the start view). All are editor seam.
- **UI:** View menu rows Frame selection (`.`), Frame all, Follow
  selection, Stop following (`menu::CAMERA_ROWS`). The `.` key is a
  `KEY_OPS` row (no clash: `Period` was unbound).
- **Feel row** (`docs/interaction-feel.md`): Algodoo play for follow,
  Blender's View Selected for frame.
- **Tests** (`tests/it/camera_views.rs`, with an orthographic camera
  spawned in the headless app): follow keeps a falling ball within 1 px of
  the centre for 2 s; frame shows every target and fits the width to the
  margin; an empty frame shows every body; look glides to its point and
  zoom and ends. `frame_view` has a unit test.

