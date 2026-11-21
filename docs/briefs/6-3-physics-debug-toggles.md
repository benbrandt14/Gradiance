# Physics debug toggles and a schedule dump
Phase 6 (testing & diagnostics) · `docs/roadmap.md` § B · status: in review
(after 6-2)

Written by the orchestrator, 2026-11-20.

## Goal

Finish the diagnostics stack of `docs/roadmap.md` § B. The `diagnostics`
and `tracy` features already exist (root `Cargo.toml`); the flight
recorder is gone. Remaining:

1. **Physics debug overlay.** avian's `PhysicsDebugPlugin` /
   `PhysicsGizmos` behind a View ▸ Debug submenu and `debug.*` operations
   (colliders, contacts, joints, AABBs, velocities), each a toggle on a
   `GizmoConfigStore` config. Workstation config, not scene content (never
   undoable, saved with the workstation config if that exists for
   similar toggles).
2. **Schedule dump.** A `gradiance -- schedule-dump [schedule]` CLI
   doorway (or an operation) that prints the system order of the main
   schedules as text or DOT, so the deferred pipeline is readable without
   a debugger. Prefer Bevy's own schedule introspection; a new crate
   (for example `bevy_mod_debugdump`) is a lockfile change: only if Bevy
   has nothing usable, and record it as a decision.

## Decide (best estimate, record in `archive.ron`)

- Which toggles ship, their defaults (all off), and where the state
  lives.
- Text vs DOT for the dump.

## Out of scope

`bevy-inspector-egui` (a separate decision), the retained-buffer gizmo
work (skipped in the roadmap), new physics behaviour.

## Done when

- Each toggle works from the menu and from its operation, with a test
  that the config flips; one lavapipe screenshot of the overlay on a
  jointed scene is attached to the PR (not committed).
- The dump runs headless in a test and lists known systems in order.
- Gates green; Outcome filled.

## Outcome

**Landed** (branch `claude/project-thread-tw1mvl-6-3`):

- **Engine debug overlay.** `gradiance_physics::debug_gizmos` adds avian's
  `PhysicsDebugPlugin` (only with a `RenderPlugin`: a gizmo group needs
  render-world assets, so headless apps get the config but no draw
  systems) and copies four `DebugSettings` switches onto its
  `PhysicsGizmos` config in `GizmoConfigStore` whenever they change:
  `show_colliders`, `show_contacts`, `show_joints` (new: anchors plus a
  line where a joint's anchors drift apart), `show_aabbs`. The group is
  disabled while all four are off, draws with `depth_bias -1` like the
  editor overlays, and never hides meshes. The store is written with
  `bypass_change_detection`: avian's `change_mesh_visibility` resets every
  rigid body's `Visibility` on a store change, which would show a ground
  the Scenery settings hid.
- The hand-rolled collider, AABB and contact overlays in
  `render::debug_viz` are removed (the engine view replaces them);
  velocities stay Gradiance's own overlay (avian 0.8 has no velocity
  gizmo), as do origins, layers, substeps, fields and authored joint
  anchors.
- **Operations** (`gradiance_interaction::debug_ops`, config seam, no
  undo step, never saved): `debug.colliders`, `debug.contacts`,
  `debug.joints`, `debug.aabbs`, `debug.velocities`, each
  `{"on": bool}` (`(debug-colliders #t)`). `DEBUG_TOGGLES` is the one
  table; **View > Debug** is a loop over it and each checkbox invokes the
  operation. No keys.
- **Schedule dump.** `gradiance schedule-dump [schedule]`
  (`src/schedule_dump.rs`): the headless app's schedules in frame order
  (`MainScheduleOrder`, the fixed loop after `RunFixedMainLoop`, the rest by
  name), each system in executor (topological) order with its
  `ApplyDeferred` sync points. Bevy's own introspection
  (`Schedule::initialize`, `Schedule::systems`); no new crate, no lockfile
  change. Render-world and window systems are absent headless.
- Tests: `tests/it/debug_overlay.rs` (defaults off; each operation flips
  its switch and the gizmo config with no undo step; the menu's
  `InvokeOp` and the Lisp verb take the same path; toggling leaves a
  hidden body hidden; the dump lists `sync_colliders` before
  `sync_joints` before `retune_joints` in PostUpdate, sections in frame
  order, an unknown name errors with the list); unit tests in
  `debug_gizmos` and `debug_ops`.
- Decisions in `archive.ron`: `debug-toggles-which`,
  `debug-toggles-state`, `debug-toggle-op-shape`, `schedule-dump-format`.

**Checked on screen** (lavapipe, `tools/screenshot`, not committed): a
pinned two-link hinge pendulum and a box on a ground with all five
switches on through the `debug.*` operations over the remote surface:
green engine collider outlines on every body and the ground line, the
AABB of the swinging link, joint anchors at both hinges, velocity marks;
no stray lines from the ground's half-space AABB.

**Caveats.** Avian's draw systems are not exercised by any test (headless
leaves them out); the View > Debug submenu itself was not opened on
screen (its rows write the same `InvokeOp` the tests send). Contact
points draw only while bodies penetrate (avian's rule), so a settled
scene shows few.

