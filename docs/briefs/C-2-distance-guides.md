# Distance guides and equal spacing while moving
Phase C · roadmap § "Unified plan" · status: ready (after C-1)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 2). Docs only.

## Goal

While a selection is moved (and while a body is drawn), measured gap lines
show its distance to the nearest neighbours on each axis, labelled in the
display unit, and the move snaps when a gap equals another gap already in
the scene (equal spacing) — the hint shows both gaps.

## Read first

- `docs/interaction-feel.md` — row **Illustrator** (smart guides).
- `crates/gradiance-interaction/src/tools/move_align.rs` — `MoveAlignment::gather`, `move_delta`, `preview_guides`.
- `crates/gradiance-geometry/src/align.rs` — `AlignFrame` (oblique axes), `align_axis`.
- VectorCraft `crates/tools/src/guides/snap.rs` `distance_guides` at `9fff48d`.

## Facts

- 1-5 aligns along the grid's axes in oblique coordinates; distance guides
  must use the same `AlignFrame` so isometric grids measure along their
  families.
- Labels go through the units display preference (5-3).

## Approach

- Feel: **Illustrator**. Pure `geometry::align::gaps(frame, moving, others)
  -> Vec<Gap>` and `equal_spacing(...)`; the overlay draws lines + labels.
- Equal-spacing snap joins C-1's priority list below alignment.
- `SnapConfig::distance_guides` (on) with a Settings row and `snap.*` op.

## Out of scope

Persistent dimensions (C-5). Distribute buttons (exist).

## Done when

- Pure tests: gap between two boxes on both axes; equal spacing found
  between three bodies; isometric frame.
- Behaviour test: moving the middle of three boxes snaps to equal gaps.
- Gates green; screenshot on the PR.

## Outcome
