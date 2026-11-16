# SSAO leaves a black strip
Phase 4 · bug in landed V1 SSAO, found by the 4-3 screenshot pass ·
status: done

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

Status: done (worker, 2026-11-16).

**Cause — confirmed in Bevy 0.19's source, not a guess.** Bevy's SSAO only
supports a camera whose viewport *is* its render target. In `bevy_pbr`
0.19: the SSAO textures are sized to `physical_viewport_size`
(`ssao/mod.rs`) and its compute passes index them viewport-locally, but the
depth/normal prepass textures are sized to `physical_target_size`
(`bevy_core_pipeline` `core_3d/mod.rs`), and `pbr_fragment.wgsl` reads the
AO with `textureLoad(screen_space_ambient_occlusion_texture,
vec2<i32>(in.position.xy))` — framebuffer coordinates, which include the
viewport offset. Our scene camera renders into the pane below the menu bar
(`ui::apply_scene_viewport`, offset 22 px), so every fragment in the last
22 rows loaded past the AO texture's height, got 0 (full occlusion), and
went black; above it the AO was shifted by the same 22 px (and the
prepass depth sampled at mis-scaled UVs whenever the pane is narrower than
the window). The grid gizmos still drew over the strip, which fits: they
are not PBR-shaded. A defect in Bevy, not in our setup — but there is a
clean workaround in our code.

**Fix (`crates/gradiance-ui/src/lib.rs`).** While the scene camera carries
`ScreenSpaceAmbientOcclusion`, `apply_scene_viewport` gives it **no
viewport** and instead a `SubCameraView` from the new pure
`full_window_sub_view(projection, viewport, window)`: an off-centre
projection under which the full-window render puts every world point on the
same window pixel the pane viewport did. SSAO then sees the one layout it
supports (offset 0, viewport = target). Orthographic and perspective need
different sub-views because Bevy updates the projection with the logical
*viewport* size (the window once the viewport is gone): ortho keeps a
window-sized full view re-centred on the pane; perspective's full view is
the pane and the window is a larger sub-rect around it. Custom projections
return `None` and keep the viewport. With SSAO off nothing changes (the
SSAO-off grab is pixel-identical to main's). The scene now also renders
under the menu bar and docks, which are opaque. Picking needed no change:
`viewport_to_world`/`world_to_viewport` take window coordinates and go
through whichever clip matrix the camera has (confirmed: a click selects
the box under the cursor with SSAO on and the right dock open).

**Test.** `ui::tests::the_full_window_sub_view_frames_like_the_pane_viewport`
— for both projection kinds, at scale factor 2, with a menu-bar and
right-dock inset, projects view-space points through the pane-viewport
clip matrix and through the full-window sub-view matrix and asserts the
same window pixel (< 0.01 px). Mutation-checked: zeroing the perspective
offset fails it. The SSAO-texture behaviour itself is GPU-side and not
testable headless; the screenshots are that evidence.

**Screenshots (scratchpad, not in git).** Before: SSAO on → black 22 px
strip along the bottom; SSAO off → none. After: SSAO on and off frame
identically, with and without the right dock (fuzz-8 % diff: only
anti-aliased edges, since SSAO turns MSAA off).

**Follow-ups (not done).**
- Upstream: Bevy's SSAO ignores camera viewports (target-sized prepass vs
  viewport-sized AO, fragment loads at framebuffer coordinates). Worth an
  issue/PR on Bevy; once fixed, the sub-view branch can go.
- Same pattern may bite other screen-space effects if added later
  (SSR, TAA, motion blur); `full_window_sub_view` is reusable for them.
- `interaction::camera::world_per_pixel` uses the window height for the
  perspective scale although the fov spans the pane height (pre-existing,
  unrelated to SSAO).
