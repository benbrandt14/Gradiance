# Brush erase mode
Phase S (sandbox parity) · roadmap § "Sandbox parity" ("brush", "erase") · status: ready · feel: Illustrator (drawing) (`docs/interaction-feel.md`)

Written by the parity audit thread, 2026-11-21, after the owner ruling "include the others" (`docs/parity-audit.md`). Docs only.

## Goal

The sketch tool gains an erase mode (hold a modifier, or a toggle in its
tool settings): the stroke, at the sketch width, is subtracted from every
body it crosses. One undo step.

## Read first

- `crates/gradiance-interaction/src/tools/sketch_tool.rs` — the stroke.
- `crates/gradiance-geometry/src/sketch.rs:185` — `sketch_shape`.
- `crates/gradiance-command/src/merge_cmd.rs` — CSG subtract.

## Facts

- The parts exist: stroke → capsule union (N-6), CSG `subtract`. A body
  split into pieces by the erase follows the cut tool's piece rules.
- Owner ruling 2026-11-21: no shape recognition. This is not recognition.

## Approach

- `body.erase` (`(erase path width)`): subtract the stroke shape from
  every body it touches, pieces as in `cut`. Sketch tool calls it in erase mode.

## Out of scope

Gesture grammar. Paint-to-add onto an existing body (E-6 owns stroke fitting).

## Done when

- Test: erasing across a box makes two pieces with the stroke area gone (±2 %).
- Undo matrix covers `body.erase`. Gates green.

## Outcome

Done 2026-11-23 (PR on the S-15 branch).

- `geometry::sketch::brush_shape`: the open capsule union of a stroke,
  never filled (a closing stroke still erases a ring, not a disc).
- `command::cut_cmd::EraseCommand` (`EraseIntent`, undo name `erase`):
  subtracts the brush from every body it touches and contours the result
  with `polygonize_components`. 0 pieces: the body goes. 1 or more: fresh
  polygon-leaf pieces through the cut's `piece_records`, joints and nodes
  rewired as for a cut. Unlike a cut, a stroke may end inside a body and
  leave a notch (decision `erase-allows-notches`). A stroke that touches
  nothing records no step. `CutCommand` and `EraseCommand` share
  `apply_stage`.
- Verb `body.erase` (`(erase path width)`), same parameters as `sketch`.
- Sketch tool: Alt held at the press makes the stroke erase (decision
  `erase-modifier-alt`); the preview draws in the danger ink; release
  invokes `body.erase`. The hint row lists "Alt+Drag".
- Tests: `tests/it/erase.rs` (a box erased across: two pieces, area within
  2 %, one undo; a notch: area within 2 %; a miss records nothing). The
  undo matrix covers `body.erase` from its example.

