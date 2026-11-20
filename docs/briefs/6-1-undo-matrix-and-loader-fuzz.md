# Undo round-trip matrix and scene loader mutation tests
Phase 6 (testing & diagnostics) · `docs/orchestration.md` § Phase 6,
`docs/unified-surface-plan.md` § 10 row 12 · status: ready

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

*(worker fills in)*
