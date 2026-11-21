# VectorCraft survey — what Gradiance takes, and where it lands

**Status: owner-approved scope (2026-11-21, in chat).** Source:
`https://github.com/storytold/vectorcraft` at **`9fff48d`** (2026-11-21).
MIT OR Apache-2.0; the ArtCraft name, logos and `docs/brand/` are reserved —
port nothing from them. Porting follows `docs/unified-surface-plan.md` § 6
(vendor in upstream shape under `crates/<crate>/src/vendor/vectorcraft/`,
record each file in `vendor/UPSTREAM.toml`, watch it in
`docs/upstream-watch.md`).

VectorCraft is one of the **Craft apps** (the family SolidCraft and
craft-libs belong to): a pure-Rust Illustrator-class editor on egui 0.37,
`kurbo` 0.13 and `vello_cpu`, ~354k lines in 23 crates, ~710 registered
commands, an MCP server and a JSON control channel. Its architecture rhymes
with Gradiance's (CI-checked layers, everything a command, tools emit
begin/preview/commit, one gesture one undo step, no panics), so its code
ports with little friction. The owner confirmed (2026-11-21) that the Craft
apps, VectorCraft included, are the "external project" the UI will be
bootstrapped from (`docs/orchestration.md` § Future intent 1).

## What Gradiance already has (no port)

- **Grids** — Gradiance is ahead: polar and isometric systems, adaptive
  steps, nesting majors (M18). VectorCraft has a rectangular grid and a
  perspective grid.
- **Number fields** — scrub, typed maths and units, Ctrl+wheel, Esc,
  middle-click reset (U-8, 5-3).
- **Booleans** — the SDF CSG tree is exact by construction; no Pathfinder.
- Align/distribute, F3 search, pies, Adjust Last, MCP and the remote
  surface.

## Owner rulings (2026-11-21)

| Item | Ruling |
|---|---|
| Bézier paths as a `ShapeDef` leaf (`kurbo`), open paths with a width become curved rods | **yes** |
| Arrays rebuilt with VectorCraft's Repeat as the reference ("a failed attempt before, but high value; good to spend time on") | **yes — rebuild** (phase T, starting with a decision, T-0) |
| Freehand shape recognition (`geom/recognize.rs`) | **no** — "this should be a more precise tool than that" |
| Dimensions publish to the signal bus | **yes** |
| VectorCraft is (part of) the external UI project | **yes** |
| All other surveyed items | **yes**, placed by the planner |
| The measure tool, rejected 2026-11-19 as "not low effort" | superseded: approved as C-4 |

## Feature map

Paths are relative to the VectorCraft root at `9fff48d`.

| # | Feature | VectorCraft source | Gradiance brief | Feel |
|---|---|---|---|---|
| 1 | One snapper for every drawing tool; targets gathered once per scene state, looked up by bisection; construction-angle guides; snap labels | `crates/tools/src/guides.rs`, `guides/snap.rs` (`DrawSnap`, `Leave`, `construction_angles`), `guides/targets.rs` | **C-1** | CAD |
| 2 | Distance guides (gap lines with lengths), equal-spacing hints while moving | `guides/snap.rs` `distance_guides` | **C-2** | Illustrator |
| 3 | Snap by the grabbed outline point (`hold_on_path`); a line snapped tangent to two round outlines (belt over pulleys) | `guides/snap.rs` `hold_on_path`; `guides/bitangent.rs`; `crates/pathops/src/addons/pathtools/tangents.rs` (`common_tangents`) | **C-3** | CAD |
| 4 | Measure tool (drag: distance, angle, dx/dy; hover: length, area, radius of curvature) | `crates/tools/src/addons/measure_inspect.rs`; `crates/geom/src/curvature.rs` | **C-4** | CAD |
| 5 | Associative dimensions → **live sensors on the bus** | `crates/tools/src/dimension.rs`, `crates/doc/src/dimension.rs` | **C-5** | CAD |
| 6 | Typed length/angle during a draft | VectorCraft shape tools' click-for-size dialog, reworked as Blender/CAD inline typing | **C-6** | CAD |
| 7 | Anchor-and-handle path model; flattening; arc length | `crates/geom/src/path.rs`, `anchors.rs`, `arc.rs`, `bez.rs` | **E-1** | — |
| 8 | Curvature tool (spline through clicked points, each three on a circle) and Pen | `crates/tools/src/draw2.rs` (`curvature_spline`, `smooth_handles`, `end_handle`), `crates/tools/src/pen.rs` | **E-2** | Illustrator |
| 9 | Direct selection: anchors, handles, segments, marquee | `crates/tools/src/direct.rs` | **E-3** | Illustrator |
| 10 | Anchor tools: add/remove keeping the shape, slide along the path, extend an open path, smooth↔corner | `crates/pathops/src/addons/keep_shape.rs`, `pathtools/slide.rs`, `pathtools/extend.rs`; `crates/tools/src/addons/slide_anchor.rs`, `extend_path.rs`, `anchor_eraser.rs` | **E-4** | Illustrator |
| 11 | Width profile (tapered open paths) | `crates/tools/src/distort/width.rs`, `crates/doc/src/appearance/profile.rs` | **E-5** | Illustrator |
| 12 | Freehand stroke to a fitted spline (sketch tool output) | `crates/pathops/src/fit.rs` | **E-6** | Illustrator |
| 13 | Cam profiles (a closed path from a radius-vs-angle curve) | — (Gradiance-native; closes M20's open "cams") | **E-7** | CAD |
| 14 | Contour (live offset) and hollow shells, with a floating bar | `crates/tools/src/addons/contour.rs`, `crates/ui-egui/src/canvas/contour_bar.rs`, `crates/pathops/src/offset.rs` | **H-1** | Illustrator |
| 15 | Live corners: per-corner round / inverted / chamfer widgets | `crates/geom/src/corners.rs`, `crates/doc/src/corners.rs`, `crates/tools/src/corners.rs`, `addons/corner_tool.rs` | **H-2** | Illustrator |
| 16 | Tangent circle to two outlines; three-point arc; smooth bridge between two circles | `crates/tools/src/addons/tangent_circle.rs`, `three_point_arc.rs`, `pathtools/bridge.rs` | **H-3** | CAD |
| 17 | Repeats: radial / grid (brick, hex, flipped rows) / mirror, with on-canvas count, radius, spacing and split handles | `crates/doc/src/pattern.rs` (`RepeatKind`, `repeat_transforms`), `crates/tools/src/repeat_radial.rs`, `repeat_grid.rs`, `mirror.rs` | **T-0 .. T-3** | Illustrator |
| 18 | Objects on a path (bodies spread along a path, turned to it) | `crates/doc/src/objects_on_path.rs`, `crates/tools/src/objects_on_path.rs` | **T-4** | Illustrator |
| 19 | Blend: N interpolated steps between two bodies | `crates/doc/src/blend.rs`, `crates/tools/src/blendwidget.rs` | **T-5** | Illustrator |
| 20 | Contextual task bar by the selection | `crates/ui-egui/src/canvas/task_bar.rs` | **B-2** | Illustrator |
| 21 | History panel | `crates/ui-egui/src/panels/history.rs` | **B-3** | Blender |
| 22 | Rulers and ruler guides as snap targets | `crates/tools/src/rulerguide.rs` | **B-4** | CAD |
| 23 | UI test harness and control-channel injection rules (compare with U-4/U-5) | `docs/control-protocol.md` § Synthetic input budgets; `crates/ui-egui/src/control.rs`; `crates/ui-egui/src/tests/` (egui_kittest) | **B-0** | — |

## Not taken

Type and fonts, print and PDF/AI/EPS/Affinity formats, colour management,
raster filters, gradient mesh, envelope/puppet/liquify warps (soft-body
territory, declined in the parity audit), the perspective grid, symbolism
sprayers, the WebAssembly plug-in host, the Illustrator script checker,
**shape recognition** (owner, 2026-11-21).

## Dependency note

`kurbo` 0.13.1 is already in `Cargo.lock` (transitively); E-1 adds it as a
direct `[workspace.dependencies]` entry (`=0.13.1`, exact-pin rule) of
`gradiance-geometry`, so the lockfile gains an edge and no new version.
VectorCraft is on `kurbo` 0.13 as well, which keeps ported path code close
to upstream.
