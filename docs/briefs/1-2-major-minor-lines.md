# Major lines that stay put
Phase 1 · M18 · "Major/minor grid lines; snap points provably on the grid at
every adaptive zoom level (3.3, 2.5)" — the major/minor half · status: done

## Goal

The grid draws two visibly distinct strata — minor lines at the adaptive
step and major lines at a coarser interval — and **major lines sit at fixed
world positions** as the user zooms. Today a "major" is every 5th *adaptive*
step, so when the step doubles, majors jump to different world positions,
and 5 × (base·2ᵏ) does not nest with the doubling lattice.

## Read first

- `crates/gradiance-render/src/grid.rs` — `draw_grid` and `line_family`
  (the every-5th rule is `grid.rs:108–116`); polar rings `grid.rs:63–83`.
- `crates/gradiance-geometry/src/snapping.rs:1–59` — `adaptive_step` and the
  `GRID_MIN/MAX_SCREEN_STEP` band; the step is always `base · 2ᵏ`.
- `docs/briefs/1-1-adaptive-snap.md` § Outcome — why renderer and snapper
  share one description of the grid; the last paragraph is this brief's
  origin.
- `docs/ui-design.md` — skim only the colour/stroke conventions, if any
  apply to overlays.

## Facts (orchestrator, 2026-10-24, on main `5bb18b7`)

- `line_family` picks colour per lattice index `i`: `i == 0` axis colour,
  `i.rem_euclid(5) == 0` minor alpha ×2.5, else minor (`grid.rs:108–116`).
  `i` counts *adaptive* steps from the origin, so majors move with zoom.
- Polar draws every ring and spoke in `minor` — no major stratum at all
  (`grid.rs:63–83`).
- Colours are hard-coded locals in `draw_grid` (`grid.rs:40–41`).
- The snapper does not care about majors; nothing in `interaction` reads
  the major interval. *Unverified:* whether any UI shows a "major every N"
  setting — grep `GridSettings` users before adding a field.

## Approach

1. Put the major-interval rule in `gradiance-geometry::snapping` next to
   `adaptive_step` as pure math (e.g. a `major_step(base, minor_step)` or a
   predicate `is_major(index, minor_step, base)`), unit-tested without the
   app. The key property: **a major line's world position is a function of
   `base` (and the authored origin/rotation) only, never of zoom** — choose
   the major interval as a power-of-two multiple of `base` so it nests with
   the doubling lattice (5× does not). Record the chosen multiple and why in
   the Outcome.
2. `line_family` asks that function instead of `rem_euclid(5)`. Origin line
   keeps the axis colour.
3. Polar: major rings by the same rule on radius; spokes unchanged.
4. Keep it a config-free change unless the unverified fact above turns up an
   existing setting. Don't add a new `GridSettings` field speculatively — if
   you think one is needed, say so in the Outcome instead.

Invariants touched: none. Nothing authored, persisted, or undoable changes.
Future intent: no new single-plane assumption (it's 1-D math on a step).

## Out of scope

Axis-lock basis (brief 1-3), snap glyphs (1-4), curvilinear coordinates,
frustum-sized grid coverage (noted in 1-1 Outcome), any settings UI,
`docs/state.md` (the orchestrator updates it; parallel workers would
conflict).

## Done when

- Geometry-crate test: for a sweep of camera scales, every line classified
  major lies at an integer multiple of the major interval from the origin,
  and that interval is identical across the sweep (fixed world positions).
- Geometry-crate test: every major line is also a drawn line at every zoom
  in the sweep (majors are a subset of the lattice).
- fmt, clippy `-D warnings`, doc `-D warnings`, `cargo test -p
  gradiance-geometry`, and `cargo test --workspace` once at the end — green.
- `docs/roadmap.md` M18 first bullet: mark the major/minor half landed.
- This brief committed in the PR with `## Outcome` filled.

## Outcome

**Landed.** `geometry::snapping` gains `GRID_MAJOR_EVERY = 4`,
`major_step(minor_step)`, and `is_major_line(index)`; `render::grid` asks
`is_major_line` for both line families and for polar rings (spokes
unchanged). The origin line keeps the axis colour. Colours unchanged in
value: minor α 0.14, major α 0.35 (the old 0.14 × 2.5), axis α 0.4.

**Chosen multiple: 4 (= 2²).** Any power of two makes the major interval
`base · 2ʲ`, which is itself an adaptive lattice: a major line at one zoom
is a drawn line at every finer zoom and still major there, and is a drawn
(minor) line at the next coarser doubling — it never moves. 5 fails because
`5 · base · 2ᵏ` lies on no coarser lattice. 4 over 8 because minors span
24–96 px on screen, so majors at 4× span 96–384 px — several stay on
screen; 8× would leave as few as two or three majors in view at the
coarse end of the band. 2 was too dense to read as a separate stratum.

**Observable consequence.** As the step doubles, a major becomes minor
(every other major survives as major), and majors appear only between
existing ones — never at new world positions off the old lattice.

**Tests (geometry crate):**
`major_lines_sit_on_a_power_of_two_multiple_of_base` (interval is
`base·2ʲ` and every major is a whole multiple of it, four bases × zoom
sweep), `major_lines_keep_their_world_positions_across_zoom` (fixed world
window; every major is drawn and major at every finer zoom, drawn at the
next doubling), `every_major_line_is_a_drawn_line` (subset of lattice).

**Unverified fact resolved:** no UI, script verb, or setting exposes a
"major every N" — `GridSettings` users are the renderer, the snapper, the
settings tab's reflect grid (spacing/origin), and persistence. No field
added. If people want to tune it, a `GridSettings.major_every` (validated
to a power of two) is the natural place; not needed now.

**Polar rings** are now drawn at `i · spacing` (multiplied) rather than by
accumulating `r += spacing`, so a major ring's radius has no drift.
