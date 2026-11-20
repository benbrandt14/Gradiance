# Undo round-trip matrix and scene loader mutation tests
Phase 6 (testing & diagnostics) · `docs/orchestration.md` § Phase 6,
`docs/unified-surface-plan.md` § 10 row 12 · status: in review

Written by the orchestrator, 2026-11-20, after Phase N closed (#291).

## Goal

1. **Undo round-trip matrix.** For every registered operation on the
   `edit` seam, from its declared example on the conformance fixture:
   dump the scene (`scene.dump`, R-1), invoke, dump, undo, dump, redo,
   dump. Assert: undo gives the first dump back exactly, redo gives the
   second back exactly, and the history depth moves by the declared step.
   The test iterates the registry, never a hand list (as
   `tests/it/op_conformance.rs` does), so a later operation is covered
   with no new test.
2. **Scene loader mutation tests.** Take every committed scene fixture
   and golden (`tests/fixtures/scenes/`, `tests/it/golden/`, review
   scenes), apply seeded mutations (truncate, drop a field, duplicate a
   `StableId`, a joint to an unknown body, NaN/inf numbers, a wrong
   `FORMAT_VERSION`, swapped enum tags, random byte flips), and load each
   through the real load path. Assert: no panic, a typed error or a
   loaded scene, and a failed load leaves the open scene unchanged.

## Facts (orchestrator, 2026-11-20, on `c874e61`)

- `op_conformance.rs` already checks that every doorway lowers a call to
  the same output and that history moves by the declared step; it does
  not compare scene content after undo and redo.
- `scene.dump` returns the `SceneRecord` as JSON; the dump mirror is
  gated and fills only for a `scene.dump` call (R-1 decision
  `gated-context-mirrors`).
- `proptest` is in the lockfile (root dev-dependency, used by
  `tests/it/csg.rs`); use it, or a seeded loop, for the mutations.

## Decide (best estimate, record in `archive.ron`)

- Which fields are excluded from the exact compare (for example derived
  values that a save never writes). Keep the exclusion list short and
  named; each entry needs a reason.
- What a duplicate `StableId` or a dangling joint does on load: reject the
  file, or repair and warn. Today's behaviour is the default unless it
  panics or corrupts state.

## Out of scope

New operations; changing undo semantics beyond a bug fix the matrix
finds (fix it, with a test, and list it in the Outcome).

## Done when

- The matrix runs over every `edit` operation and passes; any operation it
  cannot drive from its example is listed with the reason, not skipped
  silently.
- The mutation test runs at least 200 seeded cases in CI time under one
  minute and passes.
- Every bug found is fixed with a test that failed first.
- Gates green; Outcome filled.

## Outcome

Worker, 2026-11-20, branch `claude/project-thread-tw1mvl-6-1`.

**Landed:**

- `tests/it/undo_matrix.rs`: every operation that records or moves a
  history step (every `edit` operation, plus every `config` operation with
  an undo label, i.e. the scene-content settings writes; 56 today), from its
  example on the `op_conformance` fixture (helpers now `pub(crate)` there):
  `scene.dump`, invoke, dump, undo, dump, redo, dump. Undo gives the first
  dump back exactly, redo the second, and the depths move by the declared
  step at each stage. It iterates the registry. `edit.undo` is inverted by
  a redo; `edit.adjust_last` replaces the newest step, so its undo is
  compared with the fixture after one undo. Run time 17 s.
- `tests/it/loader_fuzz.rs`: every committed `.ron` scene file (8: the
  pendulum fixture, four replay goldens, the v8 format golden, two legacy v1
  snapshots) loaded as committed, then 256 seeded mutations (SplitMix64, no
  new dependency) in eight kinds: truncate, drop a field, a shared
  `StableId`, a joint or node naming a missing body, NaN/inf/1e39, a wrong
  version, a swapped enum tag, 1 to 4 bit flips. Each goes through
  `scene.load` (the File ▸ Open path) from a temp directory. Asserted: no
  panic; a typed error from `scene.load` exactly when `from_ron` errs, and
  then the scene and history are unchanged; a command refusal (bad shape)
  changes nothing; a load is exactly one `load-scene` step, stays put for
  three frames, undo gives the previous scene back, redo the loaded one, and
  it plays four frames. The as-committed files and the id mutations must
  load exactly the file's records. 33 cases load, 223 fail cleanly. Run
  time 3 s.

**Bugs found and fixed** (each with a test that failed first):

1. Pausing took one more physics step after the run was recorded:
   `pause_physics_clock` kept the running delta, which avian steps even on
   a paused clock, so the `simulate` step held a pose one step behind where
   the bodies stopped. It now zeroes the delta.
   `history::a_paused_run_records_the_pose_the_bodies_stop_at`.
2. A snapshot captured an attached node where its `Transform` was left,
   which lags its target by a frame: loading a file with a stale node
   position changed the scene a frame later and recorded a phantom
   `settings` step. `NodeRecord::capture` now derives the position with
   `NodeAttachment::follow`, the formula the follower uses (the loader
   test's as-committed load of the v8 golden).
3. A file in which two objects share a `StableId` loaded, and undoing the
   load left one of them behind. `from_ron` refuses it
   (`PersistError::DuplicateId`); unit test
   `a_file_with_a_shared_id_is_refused`.
4. A NaN in a settings field loaded, its sanitizer rewrote it a frame
   later, and a phantom `settings` step was recorded. `from_ron` refuses
   any non-finite number, found by a reflection walk
   (`PersistError::NonFinite` names the field); unit test
   `a_file_with_a_non_finite_number_is_refused`.

Since `scene.insert` and `edit.paste` read through `from_ron`, 3 and 4
apply to them too.

**Exclusions:** none of the operations is skipped. `body.mirror_v`'s
example is the identity on the fixture (every body is a circle on
y = 1), so only its "the call changed the scene" check is waived
(`IDENTITY_ON_FIXTURE`, which fails if a listed example does change the
scene). The compare leaves out the workstation config (`NOT_UNDONE`:
grid, snap, camera, workspaces), which undo never reverts by design.

**Decisions** (`docs/review/archive.ron`, `decision: true`):
`undo-compare-exclusions`, `attached-node-pose-capture`,
`duplicate-id-load` (refuse), `dangling-joint-load` (keep: loads, does
nothing, round-trips), `non-finite-load` (refuse), `loader-fuzz-corpus`.

**Noticed, not changed:** File ▸ New (`scene.new`) loads the default
environment, workstation config included, so it resets the person's saved
dock layouts and grid, and undo does not bring them back (by design of the
split; worth an owner look). A file holding a value the domain repairs on
spawn (a depth band with `near < 0`) loads repaired, so the open scene is
not byte-equal to the file; the loader test checks stability and undo for
those instead of equality.
