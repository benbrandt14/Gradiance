# Bevy 0.20 stack
Phase U · `docs/unified-surface-plan.md` § 4, § 10 row 3 ("Upgrade crates
such as egui where that prevents a lot of rework", owner 2026-11-17) ·
status: ready (after D-0)

## Goal

The workspace builds on the newest mutually compatible Bevy stack. Every
later U-brief is written once against these APIs instead of being ported
again. Behaviour does not change: the same tests pass, and screenshots of the
default scene and the main panels look the same.

## Read first

- `docs/bevy19-notes.md` (all; it becomes `docs/bevy20-notes.md`).
- `Cargo.toml` `[workspace.dependencies]` and `tests/boundaries.rs`
  (`engine_facing_dependencies_stay_exact_pinned`, line 113).
- `docs/ui-shell-decision.md` § What landed (the UI camera and the
  scene-viewport routing are the bevy_egui-sensitive parts).
- Brief 4-4's Outcome (the SSAO `full_window_sub_view` workaround).
- `tools/screenshot/README.md` (for the before/after visual check).

## Facts (planner, 2026-11-17, crates.io)

| Crate | Pin now | Target | Requires |
|---|---|---|---|
| `bevy` | `=0.19.0` | `=0.20.1` | rustc 1.97.1 (D-0) |
| `avian2d` | `=0.7.0` | `=0.8.0` | bevy ^0.20 |
| `bevy_egui` | `=0.41.0` (egui 0.35) | `=0.43.1` | bevy ^0.20, egui ^0.36 |
| `egui_tiles` | `=0.16.0` | `=0.17.0` | egui ^0.36 (0.18 needs 0.37) |
| `egui-snarl` | `=0.11.0` | `=0.12.0` | egui ^0.36 |
| `egui_plot` | `=0.36.0` | `=0.37.0` | egui ^0.36 (0.38 needs 0.37) |
| `egui_kittest` | `=0.35.0` | `=0.36.2` | |
| `leafwing-input-manager` | `=0.21.0` | `=0.22.0` | bevy ^0.20 |
| `steel-core` | `=0.8.2` | `=0.8.3` | patch |
| `glam` | `0.32` | must match Bevy: `bevy_math` 0.20.1 uses `^0.33.2` | |

- bevy_egui `main` (0.43.2) is still on egui 0.36. **egui 0.37 is not
  reachable** with a released bevy_egui. Do not fork or git-patch it (D-2
  waits for a release).
- *Unverified:* other workspace crates that depend on Bevy's versions
  (`rfd`, `lyon`, `argmin`, `colorgrad` do not; check every `bevy_*` and
  `egui*` crate in `Cargo.lock`).
- *Unverified:* the avian2d 0.7 → 0.8 changes for the authored components
  (`RigidBody`, `Friction`, `Restitution`, `ColliderDensity`,
  `GravityScale`, `Sensor`, `LockedAxes`). These are the **save format**
  (invariant 5). If a serialized shape changes, bump `FORMAT_VERSION` and
  regenerate goldens (owner ruling: format may change freely, no migrations).

## Approach

1. Bump all rows of the table in one commit; then fix the build crate by
   crate, bottom of the DAG first (`cargo check -p <crate>`).
2. Read each upstream migration guide (Bevy 0.19→0.20, avian 0.8,
   bevy_egui 0.42 and 0.43, egui 0.36) and record the changes that bit this
   codebase in `docs/bevy20-notes.md`, in the shape of `bevy19-notes.md`.
3. Keep behaviour: no feature work, no API redesign. Where an upstream API
   moved, make the smallest change and note it.
4. Visual check on lavapipe: default scene, inspector, node graph, settings,
   before and after. Describe them in the PR (owner rulings: images used by
   a test or review question may be committed; ad-hoc ones stay out).

## Out of scope

egui 0.37, a bevy_egui fork, any U-1 work (the op registry), any new
feature, removing leafwing (U-2 decides that), retained items in
`docs/state.md`.

## Done when

- Every row of the table is at its target; `tests/boundaries.rs` passes
  (exact pins).
- fmt, clippy, doc and `cargo test --workspace` pass; CI is green.
- `docs/bevy20-notes.md` exists; `CLAUDE.md` and `docs/state.md` name the new
  versions; `bevy19-notes.md` is deleted or marked as history.
- The Outcome lists each breaking change met, and whether the format version
  moved.

## Outcome

(filled by the worker)
