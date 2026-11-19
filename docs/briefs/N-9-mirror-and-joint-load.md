# Mirror and joint load colour
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: planned

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

Two small items:

1. **Mirror H / V** for the selection: an operation and an Edit-menu row
   over the existing negative scale (no key: hotkeys are not settled).
2. **Joint load colour:** the joint glyph tints from calm to hot by
   `joint-force / break_force` (or by force relative to body weight when
   the joint cannot break), so a person sees which joint will fail.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `body.scale` already says a negative factor mirrors
  (`command/ops/body.rs` ≈ line 131); joints and anchors must mirror too.
- `joint-force` is a catalog quantity; `JointCommon::break_force` exists.
  Glyphs draw in `render::joint_viz`. The tint comes from the scene look's
  colours (one colour source rule).

## Out of scope

Stress on bodies (no stress model).

## Done when

- Mirroring a hinged pair twice gives back the original scene record
  (test); a joint at 90 % of its break force draws in the hot colour.
- fmt, clippy, tests and the doc gate are green.

## Outcome

(empty)
