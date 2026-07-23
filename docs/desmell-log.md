# De-smell log

Pattern → location → action, from the maintainability pass. Doubles as the
house-style record: **kept** entries were checked and deliberately left, so
future sessions don't re-litigate them.

## Fixed

| Pattern | Location | Action |
|---|---|---|
| Tool-registration triple (11 `Tool*` variants + `tool()` match + `TOOLS` array) | `interaction/input.rs` | One `EditorAction::Tool(ToolState)` variant; a new tool touches `input.rs` once (its binding) |
| Unused symmetric API: `range_selector` | `ui/widgets.rs` | Deleted (zero callers) |
| Dead legacy compat: `BodyRecord.group` + spawn fold-in — the v3 format gate rejects every file old enough to carry it | `command/snapshot.rs` + 7 sites | Deleted |
| `resolve(world, id)`: 3 copies + ~11 inline expansions | `src/command/` | One `pub(crate) fn resolve` in `command/mod.rs` |
| Two names, one idea: `wrap_pi` vs `wrap_angle` | `physics/{grab,motor}.rs` | Unified as `geometry::wrap_angle` (differed only at exactly ±π, inside the motors' 0.05 buffer) |
| Duplicate order-preserving dedup | `interaction/` | select.rs reuses `selection::dedup_preserving_order` |
| `let _ = JointCommon::default();` doc-link no-op | `physics/joint_sync.rs` | Deleted |
| Command display names diverging from intent names | `command/*` | `name()` returns the shared `intent::name` constants |

## Kept (checked, not smells)

- `DraftTool`/`ManipTool` traits: 5+ implementors each, the scripting seam's
  landing pad — not speculative generics.
- `SelectGesture`'s 7-state machine in one resource: correctly *not* split.
- `let … else { return }` in tool/UI systems: guards legitimate runtime
  states (off-screen cursor, despawned entity), not seam-excluded ones.
- `guard_dangling_joints`: real safety net (script/test despawns bypass the
  command cascade; avian panics on dangling joints) with a `warn!`.
- `physics::queries`, `ui/reflect_grid.rs`, the command staging pattern,
  toolbar's label table: all have multiple real users / are presentational.

## Fixed in the workspace-split round (2026-07-20, `docs/workspace-plan.md`)

| Pattern | Location | Action |
|---|---|---|
| Save format split across two layers (`command::snapshot` records vs `persist::FORMAT_VERSION`/RON) — the `command ↔ persist` cycle | `command/snapshot.rs`, `persist/mod.rs` | One `gradiance-scene` package owns records + format + migrations |
| Per-intent triple registration (`add_message` + `register_type` + dispatch arm ×17) | `command/{mod,dispatch}.rs` | One `command_intents!` table row per intent generates all three |
| `EnvironmentRecord` field/capture/apply triplication (9 resources ×3 lists) | `command/snapshot.rs` | One declarative `environment_record!` macro |
| Twin commands: `SpawnBodyCommand` ≡ `SpawnNodeCommand` | `command/spawn.rs` | Generic `SpawnCommand<R: AuthoredRecord>` |
| Two hand-rolled message `drain` helpers | `command/dispatch.rs`, `persist/mod.rs` | Shared `core::messages::drain` |
| Capture-serialize-write flow ×3 (save / snapshot / autosave) | `persist/mod.rs` | One `write_scene_file` |
| `script ↔ signal` cycle through the Tier-B kernel | `script/kernel.rs` | Kernel is its own bottom crate; `signal → kernel` only |
| `geometry → domain` (the "pure" layer importing `ShapeDef`) | `domain/shape.rs` | Shape tree moved into `gradiance-geometry`; `domain::shape` re-exports |
| `interaction ↔ render` cycle through the overlay gizmo groups | `render/overlay.rs` | Groups live in `gradiance-interaction` (their writers); `render → interaction` is the one sanctioned upward-looking edge |
| Dead dependencies: `clipper2`, `rand`, `rstest` (zero references) | `Cargo.toml` | Deleted (`glam` kept, documented as features-only: serde on bevy math types) |

## Fixed in the coverage round (2026-07-20, cargo-llvm-cov + visibility probe)

Method: workspace coverage (55.3% lines — UI/render draw paths are
headless-blind by nature) cross-referenced with a compiler probe
(temporarily demote crate-local `pub` items to `pub(crate)` so rustc's
`dead_code` lint can see them; what still compiles and warns is provably
production-dead).

| Pattern | Location | Action |
|---|---|---|
| `layer_z_range` — the bit-based depth mapping retired by continuous `DepthBand` (v5); no production caller | `geometry/extrusion.rs` | Deleted (+ its legacy-convention test); extrusion docs now state the band-driven contract |
| `wrap_pi` — re-grown duplicate of `geometry::wrap_angle` (same math, same wrap) | `interaction/joint_edit.rs` | Deleted; call site uses `wrap_angle` |
| `Expr::eval_ref`, `Kernel::stack_depth` — proptest oracle/diagnostic in the shipped API | `kernel/src/lib.rs` | Demoted to `#[cfg(test)] pub(crate)` (tests keep their oracle; product surface shrinks) |
| `Triangulation::area` — test-only coverage checksum (production reads `Contours::area`) | `geometry/tessellate.rs` | Demoted to `#[cfg(test)] pub(crate)` |

## Kept (uncovered, checked, not dead)

- `reflect_bridge::{scalar_to_steel, reflect_to_steel}`: production-unused
  today but the spike-1-validated "reads are total" conversion the P2
  plotter/driver work binds to (`docs/script-spike-findings.md`); tested.
- `intent::name::{UNDO, REDO}`: used by the command-dispatch undo/redo trace (a
  featureless probe run flags them — false positive).
- UI/render draw paths (185 uncovered fns): gizmo/egui code needs a
  windowed session; not reachable from the headless suite.
- `physics::queries` facade entries without callers yet: the read-total
  contract says the facade stays complete.

## Follow-ups

- Move `ui/plot.rs::joint_signals` behind `physics::queries` when the next
  plottable signal lands (see `docs/recipe-audit.md`).

## Net LOC

The de-smell changes are net-negative in `src/`. The pass's net-positive is
the flight recorder (dev-gated, zero default-build cost) plus the
`edit_bindings` validation table — the observability deliverables.
