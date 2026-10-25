# Line things up while moving them
Phase 1 · M18 · "Light collinear/centerline snapping while dragging
('lightweight assemblies') — configurable in the snapping menu (3.1)" ·
status: ready

## Goal

While a selection is dragged with the select tool, it lightly snaps so its
edges line up with other bodies' edges (collinear) and, optionally, its
centre lines up with other bodies' centres (centrelines), along the **active
grid's axes**. A faint guide line shows each alignment that is holding.
Both behaviours are toggles in the existing Grid & Snap → Snapping section.
Feedback 3.1: "mostly just to make objects co-linear when dragged together
… Should lightly snap to collinear on move/create, should optionally snap
to centerlines."

## Read first

- `crates/gradiance-interaction/src/tools/select.rs` — `SelectGesture::Move`:
  `start_move` (≈L400–420), `drag` (≈L453–470), `release` (≈L560–572). The
  preview and the commit both compute `delta = constrain(p - start)`.
- `crates/gradiance-interaction/src/gesture.rs` — `grid_basis` and
  `GestureConstraints::basis` (brief 1-3): the axes alignment measures along.
- `crates/gradiance-interaction/src/tools/context.rs` — `ToolWorld` (≈L499:
  `shape_pose`, `bodies_in_box`) and `ManipContext`/`ManipOutput`.
- `crates/gradiance-geometry/src/snap_features.rs` (brief 1-4) — circles are
  read analytically there; reuse it for a shape's extent along an axis
  rather than polygonizing circles.
- `crates/gradiance-domain/src/settings.rs` ≈L85–111 — `SnapConfig`.

## Facts (orchestrator, 2026-10-25, against main after #213–#215)

- `SnapConfig` derives `Serialize, Deserialize, Reflect` and is persisted
  with the scene (`scene/src/records.rs` ≈L404) as workstation config
  (invariant 4: saved, never undoable). New fields need `#[serde(default)]`
  so old scenes load.
- The Snapping UI is `reflect_grid_units` over `SnapConfig`
  (`ui/src/settings.rs` ≈L180), so new bool fields appear in the snapping
  menu with **no UI code** — prefer that.
- Move preview and commit share the delta computation, so if alignment
  adjusts `delta` in one helper used by both, preview == commit (invariant
  2: one commit on release). Today drag and release each recompute it.
- `GestureConstraints::basis` holds the lock axes (the grid's line families
  when visible, world X/Y otherwise). Isometric axes are not orthogonal —
  measuring an extent "along an axis" means projecting onto it.
- A held axis lock (`X`/`Y`) zeroes motion along one axis; alignment along
  that axis must not reintroduce it. *Unverified:* the cleanest rule —
  probably "only align along axes the lock leaves free".

## Approach

1. **Pure math in geometry** (new module, e.g. `geometry::align`): given the
   moving set's extent intervals along each basis axis, the other bodies'
   intervals, a tolerance in world units, and which features are enabled
   (edges: min/max; centres: midpoint), return a per-axis correction (the
   smallest one within tolerance, or none) plus which reference matched.
   Unit-test it without the app, including the isometric case.
2. **Extents** per body along an axis come from the shape: circles from
   `snap_features` (centre ± r), everything else from polygonized vertices
   (polygonize is the single discretization point; don't contour anything
   new). Candidate bodies: those near the moving selection (`bodies_in_box`
   around its AABB grown by a margin), excluding the moving bodies.
   Compute the reference set **once at gesture start** (bodies don't move
   relative to each other while you drag in pause mode) — keep the frame
   cost flat.
3. **Wire into Move**: one helper computes `delta = align(constrain(p -
   start))`, used by both `drag` and `release`. Tolerance is "light":
   suggest half of `max_screen_distance`, converted with `cam_scale`.
4. **Guides**: draw a faint line along each holding alignment, through the
   matched reference, via the existing manip preview path.
5. **Config**: `SnapConfig` gains two bools, e.g. `align_edges` (default
   true) and `align_centers` (default false), `#[serde(default)]`.
   Alignment is skipped entirely when both are off or `objects_enabled` is
   off (record which you chose in the Outcome).

Invariants: 2 (preview only during drag; exactly one `Move` commit on
release) — keep it; 4 (SnapConfig stays workstation config). Future intent:
alignment reads axes from the grid frame; no new hardcoded XY.

## Out of scope

Alignment during **creation** tools (cursor tracking — a later brief, note
what you learn), duplicate-drag/array gestures, rotation alignment, CSG-arc
snap features (separate follow-up), any new panel or bespoke UI, the
tangent snap (1-7), `docs/state.md` (the orchestrator updates it).

## Done when

- Geometry-crate tests: edge and centre alignment within tolerance snaps,
  outside does not; the smallest correction wins; isometric basis; disabled
  features never match.
- Integration test in `tests/it/interaction.rs`: two boxes; drag one so its
  bottom edge ends within tolerance of the other's; the committed move puts
  the edges exactly collinear. A second case with `align_edges` off commits
  the raw delta.
- An old scene RON without the new fields still loads (existing
  persistence tests may already cover `serde(default)`; extend one if not).
- fmt, clippy `-D warnings`, doc `-D warnings`, per-crate tests green;
  `cargo test --workspace` left to CI if disk is tight (say so).
- `docs/roadmap.md` M18 collinear bullet marked landed (creation half
  noted as open). This brief committed with `## Outcome` filled.

## Outcome

*(worker fills in)*
