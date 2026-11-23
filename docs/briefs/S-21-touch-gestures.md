# Trackpad and touch gestures
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("multi-touch") · status: done · feel: Blender (navigation) (`docs/interaction-feel.md`)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

Pinch zooms, two-finger drag pans, two-finger rotate orbits, on trackpads
and touch screens. Camera only; editing gestures stay mouse-based.

## Read first

- `crates/gradiance-interaction/src/camera.rs:51` — `CameraRig`.
- `crates/gradiance-interaction/src/pointer.rs` — current pointer input.

## Facts

- Bevy has `PinchGesture`/`RotationGesture` (macOS trackpads) and
  `Touches`. *Unverified* coverage on Linux and Windows in Bevy 0.20;
  record what each platform gets.

## Approach

- One system maps gestures to the rig, scaled by `CameraSettings`.

## Out of scope

Kinetic panning. Touch editing.

## Done when

- Test with injected gesture events: zoom and pan move the rig as expected.
- Gates green.

## Outcome

Done 2026-11-23 (PR on the S-20 branch).

- `interaction::camera_gestures::touch_and_trackpad` runs after
  `pan_and_zoom_camera`. Two touch fingers pan, zoom about their midpoint
  (spread ratio) and roll the view. Trackpad pixel scroll pans,
  `Ctrl` + pixel scroll zooms, `PinchGesture` zooms about the cursor,
  `RotationGesture` rolls, `PanGesture` pans. Off-UI only, and not during
  a tool gesture.
- `camera.rs`: the wheel now zooms only on line-unit scroll. New shared
  helpers `zoom_about` (anchored zoom, both projections) and
  `pan_by_pixels`.
- Defect found: Bevy updates `Touch::previous_position` only on a frame
  with touch events, so fingers at rest would repeat their last motion.
  The system keeps the pair's last positions by finger id instead.
- Platform coverage (from the Bevy and winit docs, not tried on
  hardware) is a table in the module docs. In short: macOS gets every
  trackpad gesture; Windows and X11 touchpads scroll in lines, so their
  two-finger drag still zooms; touch screens work on Windows, Linux and
  iOS.
- Decisions `scroll-unit-splits-wheel-and-trackpad`,
  `two-finger-turn-rolls` (the brief said "orbits"; a turn on a flat
  screen rolls), `gesture-zoom-not-scaled`.
- Tests: `tests/it/touch_gestures.rs` injects `TouchInput`, `MouseWheel`
  (pixel and line), `PinchGesture` and `RotationGesture`: pan 0.5 m,
  zoom 2x, roll 45 degrees, rest holds, no undo step. Unit tests for the
  two-finger math.
- Not checked: real hardware. The cursor anchor is untested headless (no
  window), so the zoom there is about the focus.
