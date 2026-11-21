# Rulers and ruler guides
Phase B · roadmap § "Unified plan" · status: ready (after C-1; native)

Written by the planning thread, 2026-11-21 (`docs/vectorcraft-survey.md` row 22). Reverses the 2026-11-19 rejection of rulers (owner, 2026-11-21). Docs only.

## Goal

Optional rulers along the viewport's top and left edges, in the display
unit, following the grid frame (rotation, origin, user coordinate system).
Dragging out of a ruler makes a guide line; guides are C-1 snap targets
(`guide` label), can be selected, moved, locked and deleted, and are
workstation config (saved with the scene, never undone — like the grid).

## Read first

- `docs/interaction-feel.md` — row **CAD**; CLAUDE.md invariant 4 (workstation config vs scene content; classify in `scene_content!`).
- `crates/gradiance-interaction/src/snap.rs` — where guides join the target list.
- VectorCraft `crates/tools/src/rulerguide.rs` at `9fff48d`.

## Approach

- Feel: **CAD**. `GuideSettings { guides: Vec<Guide { axis, offset, locked }> }`
  in grid-frame coordinates; rulers drawn by `gradiance-ui` from a mirror;
  `guide.*` operations. Not on the undo stack.

## Out of scope

Guides from arbitrary angles (a later option); artboards.

## Done when

- Behaviour test: a box drawn near a guide snaps onto it; File ▸ New keeps
  guides (Q-3's rule for workstation config) or clears them, as decided and
  recorded.
- Gates green; screenshot.

## Outcome
