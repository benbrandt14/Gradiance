# egui 0.37 (was D-2)
Phase B · roadmap § "Unified plan" · status: **trigger-driven** — ready when bevy_egui publishes a release on egui 0.37

Written by the planning thread, 2026-11-21. Supersedes the D-2 line in `docs/unified-surface-plan.md` § 13. Docs only.

## Goal

Move the egui family to 0.37 so Craft-app code (VectorCraft, craft-libs,
SolidCraft; all on 0.37) vendors without a version shim.

## Facts

- 2026-11-21: crates.io has `bevy_egui` 0.43.1 (egui 0.36); the workspace
  pins egui 0.36.2 through it, with `egui-snarl` 0.12, `egui_tiles` 0.17,
  `egui_plot` 0.37 (on egui 0.36), `egui_kittest` 0.36.2.
- VectorCraft builds against a fork of egui at a pinned git rev; Gradiance
  takes crates.io releases only.

## Approach

- One PR: bump `bevy_egui` and every egui-family pin in
  `[workspace.dependencies]` together (exact pins; `tests/boundaries.rs`),
  regenerate `Cargo.lock` as the reviewed change, fix breakage, re-run U-5
  snapshots (regenerate goldens with the reason).
- If `egui-snarl` or `egui_tiles` lag, stop and hand back with the list.

## Out of scope

Any chrome change.

## Done when

- Gates green; snapshot diffs explained on the PR.

## Outcome
