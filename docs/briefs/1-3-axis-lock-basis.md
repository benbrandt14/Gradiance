# Axis lock follows the grid
Phase 1 · M18 · "Axis-lock basis follows the active grid system (2.5)" ·
status: ready

## Goal

Holding `X`, `Y`, or `X`+`Y` constrains a move to the **active grid's
axes**, not world X/Y. On a rotated Cartesian grid, `X` locks along the
grid's rotated first axis; on an isometric grid the locks follow its two
line families (30°/150° rotated by `grid.rotation`); the dominant lock picks
whichever *grid* axis the delta favours. The blue constraint guide lines draw
along the same axes, so what is drawn is what is enforced. Feedback 2.5:
"The dominant axis should match the basis of whichever grid is active."

## Read first

- `crates/gradiance-interaction/src/gesture.rs` — `AxisConstraint`,
  `GestureConstraints::constrain`, `update_gesture_constraints`.
- `crates/gradiance-geometry/src/snapping.rs:15–22, 139–155` — the shared
  line-direction constants and `constrain_axis`.
- `crates/gradiance-interaction/src/indicators.rs` — `draw_constraint_guides`
  (world-axis guide lines today).
- `crates/gradiance-interaction/src/tools/select.rs` — the four
  `ctx.constraints.constrain(...)` call sites (≈L209, 455, 562, 633).

## Facts (orchestrator, 2026-10-24, on main `5bb18b7`)

- `constrain_axis(delta, horizontal, vertical, dominant)` zeroes world
  components (`snapping.rs:141–155`); `GestureConstraints::constrain` maps
  the enum onto it (`gesture.rs:48–55`) with no access to `GridSettings`.
- All four consumers are in `tools/select.rs` and go through
  `ctx.constraints.constrain(..)`. *Unverified:* whether `ToolContext`
  already carries `GridSettings` — check before threading it.
- `draw_constraint_guides` draws `Vec2::X`/`Vec2::Y` guides
  (`indicators.rs` ≈L62–85).
- `CARTESIAN_LINE_DIRECTIONS` / `ISOMETRIC_LINE_DIRECTIONS` are unit
  vectors in the unrotated grid frame; rotate by `grid.rotation` — the same
  description the renderer and snapper use (brief 1-1 Outcome).
- Polar grid has no straight line families. Pick and document a rule (e.g.
  radial/tangential about the grid origin at the gesture start, or world
  axes rotated by `grid.rotation`) — record the choice in the Outcome.

## Approach

1. Pure math in geometry: project a delta onto a basis (two unit axes) —
   `constrain_to_basis(delta, axes, AxisMode)` or similar — and a function
   that yields the active basis for a grid system + rotation. Unit-tested
   without the app. Note isometric axes are **not orthogonal**: "lock to
   axis A" is projection onto A's direction (keep the component along A),
   and "dominant" compares the absolute projections. Write down the
   definition you chose.
2. `GestureConstraints::constrain` (or its caller) receives the basis;
   whether the grid's visibility/snap flags matter is your call — say which
   in the Outcome (suggestion: basis follows the grid whenever the grid is
   visible, world axes otherwise).
3. `draw_constraint_guides` draws along the same basis.
4. Keep `constrain_axis` only if something still needs it; otherwise remove
   it and its test rather than leaving a dead twin.

Invariants: 2 (tools write preview only during drag; one commit on
release) — unchanged, the constraint just changes direction. No authored
state changes. Future intent: the basis comes from the grid's frame, the
one place "the plane's axes" live — don't hardcode a new XY assumption.

## Out of scope

Major/minor lines (1-2), snap glyphs/snap engine (1-4), key bindings,
curvilinear tool coordinates, `docs/state.md` (orchestrator updates it).

## Done when

- Geometry-crate tests: Cartesian rotated 30° — `X` lock output is parallel
  to the rotated x axis; isometric — each lock is parallel to its family
  direction; dominant picks the larger projection; unrotated Cartesian
  reproduces the old world-axis behaviour exactly.
- An integration test in `tests/it/interaction.rs`: with a rotated grid
  visible and `X` held, a select-tool drag commits a move parallel to the
  grid axis.
- fmt, clippy `-D warnings`, doc `-D warnings`, per-crate tests, and
  `cargo test --workspace` once at the end — green.
- `docs/roadmap.md` M18 axis-lock bullet marked landed; module docs in
  `gesture.rs` updated (the modifier table says "horizontal-only").
- This brief committed in the PR with `## Outcome` filled.

## Outcome

*(worker fills in)*
