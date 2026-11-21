# Trackpad and touch gestures
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("multi-touch") · status: ready · feel: Blender (navigation) (`docs/interaction-feel.md`)

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
