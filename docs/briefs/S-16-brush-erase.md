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
