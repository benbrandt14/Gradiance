# Packing as an operation
Phase R · roadmap § "Run & read surface" · status: in review

Written by the planning thread, 2026-11-19. Docs only.

## Goal

`(pack targets)` / `pack.run` starts the layout solver on the listed
bodies (indexed, id order) and commits one transform command when it
finishes, as the Optimizer panel does. A query reads the run's status and
report. The optimizer is then reachable from Lisp, the CLI and MCP.

## Facts (planning thread, 2026-11-19, on `92ab302`)

- `interaction::pack` drains `StartPackRequest` (a unit struct) and
  gathers the **selection**. Script rule 2 forbids a verb that reads the
  selection, so the request must carry targets (`Vec<StableId>` or
  indices resolved in the op's context); the panel then sends the
  selection's ids through the same field.
- `PackSession` already exposes `is_active`, `status`, `report`.
- `PackConfig` is a `Resource` + `Reflect`: a `pack.set path value`
  config operation follows the `sim.set` pattern if wanted.

## Out of scope

New solver strategies; packing during play.

## Done when

- One run from Lisp and one from the panel give the same committed
  layout for the same targets and config (test).
- fmt, clippy, tests and the doc gate are green.

## Outcome

Worker, 2026-11-19, branch `claude/project-thread-tw1mvl-r4`.

**Landed (six operations, all in `docs/ops.md`):**

- `crates/gradiance-interaction/src/pack.rs`: `StartPackRequest` now
  carries `targets: Vec<StableId>`; `start_pack` resolves them through
  `IdIndex` and no longer reads the selection (script rule 2). The ids are
  sorted and de-duplicated first, so one set of bodies is one problem from
  every doorway (decision `pack-target-order`). `PackSession` keeps the
  ended run's report as `PackOutcome` (`last_outcome()`), cleared at the
  next start.
- `crates/gradiance-interaction/src/pack_ops.rs`: `pack.run` (`pack
  targets`; edit seam, declares `commit-transform`; resolves targets with
  `command::ops::body_set`, fewer than two bodies is a no-op),
  `pack.status` (query; a JSON object with `state` and the report),
  `pack.apply` / `pack.cancel` (editor; lower to `PackControl`),
  `pack.get` / `pack.set` (`PackConfig` by reflect-path; config seam,
  workstation config, never undone). The mirrors (`PackConfig`,
  `PackMirror`) are filled when an operation context is built, never per
  frame.
- `crates/gradiance-ui/src/optimizer.rs`: the context menu, the Properties
  strip and the Optimizer window invoke `pack.run` with `["selection"]`,
  `pack.apply` and `pack.cancel` (`InvokeOp`) instead of writing the
  messages. A panel run reaches the session one frame later than before.
- Tests (`tests/it/packing.rs`): `a_lisp_pack_and_a_panel_pack_commit_the_same_layout`
  (the "done when": a Lisp `(pack (list 4 2 0 1 3))` and the panel's
  `pack.run` on a reversed selection commit equal poses, one
  `commit-transform` step each, for Shelf and Descent);
  `pack_status_reads_the_run_and_apply_commits_a_held_one` (idle → `pack-set
  auto_apply 0` → finished and nothing written → `pack-apply` one step,
  `applied` with shrinkage → cancel leaves the poses, `cancelled`; one body
  is a no-op, a bad index an error). The older session tests write the
  request with the selection's ids (same frame timing as before).
  `op_conformance` covers the six operations from their examples.
- Docs: `docs/scripting.md` § "Packing", `docs/ops.md` regenerated.

**Stray test output (orchestrator finding):** `every_scripting_doc_example_lands_on_screen`
ran R-1's example `(save-scene "swing.ron")` / `(signal-export "swing.csv" …)`
with the repository as working directory, so every run rewrote the tracked
root files `swing.ron` and `swing.csv`. The test now redirects the path of
`save-scene` and `signal-export` into a per-process temp directory (removed
after; `file_writing_examples_write_into_the_scratch_directory` pins the
rewrite), and both files are `git rm`ed. No other R-1..R-3 file landed at
the root (checked the files added since 3f211ab; the op examples write under
`target/`). Decision `doc-example-outputs`.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`pack-target-order`, `pack-op-seams`, `pack-status-shape`,
`doc-example-outputs`.

**Per-frame cost:** none added. The session systems are unchanged (they
return at once with no run); the status and config mirrors run only when an
operation context is built.

**Not done / follow-ups:**

- The solver, objective and other enum fields of `PackConfig` have no
  operation (reflect-path writes are numbers and bools); the Optimizer
  window sets them. A `pack.solver name` op is a small later row.
- In Lisp the status object arrives as JSON text (the bridge's rule for
  objects); CLI and MCP clients get the object.
- Not checked: the panel buttons in a real window (the panel path is
  tested through the `InvokeOp` it writes).
