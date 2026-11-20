# Laser node
Phase N (nonlinear dynamics & multibody play) · roadmap § "Nonlinear dynamics & multibody play" · status: in review

Written by the planning thread, 2026-11-19, from the owner's approved
brainstorm. Docs only.

## Goal

A placeable node that casts a ray along its axis, reflects off bodies up
to N times, and draws the beam. It publishes a bus value: 1 while the
beam reaches a named target body, else 0, and the hit distance. Light
gates and reflection puzzles then work through N-1.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- avian `SpatialQuery` ray casts are available in `physics`; the read
  goes into `physics::queries` so scripts get it too.
- Nodes: see N-3. Beam drawing uses the overlay gizmo group.

## Out of scope

Refraction and colour mixing.

## Done when

- A beam at 45 degrees onto a mirror wall hits a target body placed by
  geometry (behaviour test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-n8`.

**Landed:**

- `gradiance-geometry/src/beam.rs` (new, pure): `reflect` (`d − 2(d·n)n`,
  either normal sign) and `trace_beam`, the bounce loop over a caller's ray
  cast: it writes the path into a reused buffer, stops on an absorbing hit
  (the target), at the bounce limit, at the range, or on a hit with no
  normal; unit tests against analytic mirrors (the 45-degree mirror case,
  the range, two facing mirrors at the cap, degenerate beams).
- `gradiance-domain/src/laser.rs` (new): `Laser { name, target, angle,
  range, bounces }` (default `"laser"`, no target, local +x, 50 m, 8
  bounces; caps `MAX_BOUNCES` 32 and `MAX_RANGE` 10 km);
  `NodeKind::Laser`. Additive variant: old scenes load, no format bump; the
  v8 golden gains a laser node (regenerated).
- `gradiance-physics/src/laser.rs` (new): the derived `LaserBeam`
  component (path, hit, distance, the two bus names) and `trace_lasers`,
  one system in `FixedPostUpdate` after avian's step (every fixed tick,
  paused too). `physics::queries::cast_beam` is the read: `trace_beam` over
  avian's `SpatialQuery::cast_ray_predicate`, skipping sensors and the
  laser's own body, masked by the riding body's collision layers.
- Signal: `publish_lasers` (before `evaluate_computed`) publishes `<name>`
  (1/0) and `<name>.distance`; bus hygiene keeps live lasers' names.
- Command: `node.laser` / `(laser body x y angle target name)` (`-1` for a
  free laser or no target; one `spawn-node` step); `remap_node` aims a
  copied laser at a copied target (duplicate and insert). `docs/ops.md`
  regenerated.
- Interaction: `ToolState::Laser`, `LaserTool` (press, drag to aim, release
  over another body to target it; a click aims along +x), `tool.laser` with
  no chord, hints. No editor key binding changed.
- Render: the laser glyph has a barrel along its axis; the beam draws in
  the danger ink with an accent ring where it reaches the target.
- UI: palette entry "Laser" (Modify group, `laserpen.png`, no key);
  outliner label; the node editor (inspector, node menu, Signals list)
  edits the bus name (commits on Enter or focus loss), axis, range and
  bounces, and clears the target.
- Tests: `tests/it/lasers.rs` —
  `a_beam_at_45_degrees_off_a_mirror_wall_hits_the_target` (the "done
  when": a static floor as the mirror wall, a free laser at (0, 2) aimed
  down at 45 degrees, a disc of radius 0.25 at (4, 2) on the reflected ray;
  the path turns at (2, 0) within 1 mm, the bus reads 1 and the distance
  4√2 − 0.25 within 1 cm), `a_target_off_the_reflected_ray_reads_zero`,
  `a_body_across_the_beam_blocks_it`, `the_bounce_limit_caps_the_path`,
  `the_hit_signal_drives_an_actuator` (N-1 gravity-scale drive from the
  gate), `a_laser_edit_is_one_undo_step`,
  `the_laser_is_saved_and_the_beam_is_not`,
  `the_laser_operation_places_a_targeted_laser`,
  `duplicating_a_laser_and_its_target_retargets_the_copy`. Unit tests in
  geometry, domain, physics, interaction.
- Docs: `docs/signal-dataflow.md` (laser section, `LaserBeam` row),
  `docs/scripting.md` (`laser`).

**Per-step cost:** `trace_lasers` is `O(L × (B + 1))` ray casts per physics
step for `L` lasers with at most `B ≤ 32` bounces each, each cast a BVH
traversal of avian's collider trees; per laser two `IdIndex` probes (its
body, its target). No allocation in the steady state: the path buffer is
sized once for the cap, the bus names are rebuilt only on a rename.
`publish_lasers` is `O(L)` bus publishes per frame (a name scan each), and
the bus hygiene adds an `O(L)` check per bus entry.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`laser-reflects-everything`, `laser-signals`, `laser-step`, `laser-tool`,
`laser-operation`, `laser-duplicate`.

**Not done / follow-ups:** not checked on screen (no screenshot run): the
beam, the glyph and the palette icon. No laser block on the node canvas
(the values are read as named bus signals). The target can only be set by
the tool or the operation (the editor can clear it). While paused the beam
casts against the collider trees as the last step left them, so a body
moved while paused shows in the beam after the next step. Two lasers with
one name share one bus value. No ray-cast script verb.
