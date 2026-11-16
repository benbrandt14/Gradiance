# SSAO leaves a black strip
Phase 4 · bug in landed V1 SSAO, found by the 4-3 screenshot pass ·
status: ready

## Goal

With SSAO on, the 2D view shows a black strip along its bottom edge, the
height of the top menu bar (≈22 px). With SSAO off there is none. After this
brief, turning SSAO on changes shading only, never the framing.

## Read first

- Brief 4-3's Outcome: the SSAO/MSAA blank-viewport fix,
  `render::scenery::match_overlay_msaa`, and the strip follow-up.
- `crates/gradiance-render/src/scenery.rs` and wherever the scene camera's
  `Viewport` is set from the egui menu-bar height (grep `Viewport`,
  `physical_position`).
- `docs/bevy19-notes.md`; Bevy 0.19's SSAO plugin source in
  `~/.cargo/registry` (`bevy_pbr` … `ssao`), for how it sizes its
  textures/views relative to the camera viewport.

## Facts (orchestrator, 2026-11-16, main `d459f06`)

- SSAO is off by default (a Lighting toggle, V1).
- *Unverified:* the 4-3 worker's guess that SSAO meets the camera's viewport
  offset, e.g. a depth/normal prepass sized to the full target while the
  main pass uses the offset viewport. Confirm the cause before fixing.
- If the defect is in Bevy itself and has no clean workaround in our code,
  that is a valid outcome. Document it, keep the smallest safe mitigation
  (e.g. no viewport offset while SSAO is on, if that framing is acceptable),
  and say so.

## Approach

1. Reproduce via `tools/screenshot/`: SSAO on, a scene with a ground, grab;
   SSAO off, grab.
2. Find the cause. Fix it in our render setup where possible.
3. A test if the cause is in our code and testable headless (e.g. the
   viewport/size we hand SSAO); otherwise the before/after screenshots are
   the evidence, and the Outcome says why no test.

## Out of scope

Tuning SSAO strength or radius (the near-invisible AO at scene scale is a
design call; retained), other render changes, `docs/state.md`.

## Done when

- Before/after screenshots described in the PR (not in git); cause in the
  Outcome. Gates green. Outcome filled.

## Outcome

*(worker fills in)*
