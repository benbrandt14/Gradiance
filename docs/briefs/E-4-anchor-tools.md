# Anchor tools: add, remove (keeping the shape), slide, extend, smooth/corner
Phase E · roadmap § "Unified plan" · status: ready (after E-3)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 10). Docs only.

## Goal

In Direct Selection: double-click a segment to add an anchor on the curve
(the shape unchanged); `Delete` on selected anchors removes them with the
neighbours' handles refitted so the outline stays within a tolerance; a
modifier-drag slides an anchor **along** its curve; dragging an open path's
end extends it the way it runs (straight, or continuing the last curve);
`Alt`-click on an anchor toggles smooth ↔ corner. Each is one command.

## Read first

- `docs/interaction-feel.md` — row **Illustrator**.
- `docs/briefs/E-3-direct-selection.md` — the tool and `ReshapeIntent`.
- VectorCraft `crates/pathops/src/addons/keep_shape.rs`, `pathtools/slide.rs`, `pathtools/extend.rs`; `crates/tools/src/addons/slide_anchor.rs`, `extend_path.rs` at `9fff48d`.

## Facts

- Splitting a cubic at `t` (de Casteljau) keeps the shape exactly; kurbo
  provides `subsegment`.
- Removing an anchor while keeping the shape is a least-squares refit
  (VectorCraft's `keep_shape` uses its `fit` module).

## Approach

- Feel: **Illustrator**. Pure functions in `geometry::path`
  (`insert_anchor`, `remove_keeping_shape`, `slide_anchor`, `extend_end`,
  `toggle_smooth`), vendored where VectorCraft's are copied; all emit
  `ReshapeIntent`.
- Operations `path.insert`, `path.remove`, `path.slide`, `path.extend`,
  `path.smooth` with indexed targets.

## Out of scope

Width (E-5). Trim/split at crossings (cut tool exists).

## Done when

- Pure tests: insert keeps the curve (sampled distance < 1e-5 m); remove
  of a redundant anchor keeps the outline within tolerance; slide keeps the
  outline; toggle twice is identity.
- Gates green.

## Outcome
