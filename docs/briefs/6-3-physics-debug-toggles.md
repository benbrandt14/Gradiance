# Physics debug toggles and a schedule dump
Phase 6 (testing & diagnostics) · `docs/roadmap.md` § B · status: ready
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

*(worker fills in)*
