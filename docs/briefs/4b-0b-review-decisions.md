# Review mode carries plan decisions
Phase 4b · owner direction 2026-11-17: "pull in the answers.ron from main,
and utilize/extend this review mechanism directly for as many decisions as
possible" · status: done (PR open; see Outcome)

## Goal

Review mode (4b-0) asks product questions as live scenes. The owner wants
plan decisions to go the same way: an order of work, an architecture option,
a process rule. No scene can show those, and 4b-0's validation rejects an
option without a scene. After this brief a question can be **text-only**,
the three answers on `main` are ingested, and the open decisions of the
current session are in `docs/review/questions.ron`.

## Read first

- `docs/orchestration.md` § Review mode.
- `crates/gradiance-scene/src/review.rs` (`Question::validate`).
- `crates/gradiance-ui/src/review.rs` (`question_view`).
- `tests/it/review.rs`.

## Facts (2026-11-17, `main` `2463e31`)

- `Question::validate` fails an option with neither `scene` nor `scripts`.
  The loader (`src/review.rs` `option_steps`) already shows an empty scene
  for such an option.
- `tests/it/review.rs` runs the mechanism tests over the **live**
  `docs/review/questions.ron` and names `shading` and `startup-scene`. The
  documented ingestion step (delete the question's entry) thus breaks three
  tests. 4b-0 did not meet this, because nothing was ingested before.
- `answers.ron` on `main` holds three answers: `fillet-bulge` (accept),
  `shading` (toon), `startup-scene` (empty).

## Approach

1. `Question.decision: bool` (`#[serde(default)]`, no format version bump).
   A decision question's options have a caption and no scene, scripts or
   settings. The panel lists every option with its caption and shows no
   switcher.
2. Move the mechanism tests onto a frozen fixture,
   `tests/fixtures/review/questions.ron` (the three seed questions; it uses
   the archived scenes). A second test loads every option of the live file.
3. Ingest the three answers by the documented steps.
4. Add the open plan decisions as `decision: true` questions.
5. Update `docs/orchestration.md` § Review mode with the rules for decision
   questions.

## Out of scope

A "recommended" field, per-question links, markdown in captions, any change
to the answers format, option-level camera (retained in `docs/state.md`).

## Done when

- A `decision: true` question validates, loads and can be answered.
- The mechanism tests pass with any content of the live questions file.
- `answers.ron` is empty, `archive.ron` holds the three entries, and each
  answer is a ruling or a brief change.
- fmt, clippy, doc; CI green.

## Outcome

Worker session, 2026-11-17, branch `claude/review-decisions`.

- **Landed:** steps 1 to 5. Six decision questions are open: `order-4b-1`,
  `op-output`, `op-context`, `op-decoder`, `op-keys`, `toolchain-file`.
  Each names the PR that holds its recommended option (#239 or #240).
- **Ingested as:** see `docs/review/archive.ron` (`ingested_as`) and
  `docs/state.md` (rulings; retained UI/render). `shading` changed brief
  4b-3 (new "Owner input" section).
- **Decision recorded here, not asked:** the recommendation is a
  convention ("Recommended." at the start of one caption), not a schema
  field. A field can follow if the owner wants the panel to mark it.
- **Not verified locally:** `tests/it/review.rs` and the panel. The owner
  ruled that long runs go to CI; the `it` binary is one. No screenshot of
  the decision panel was taken. Local: `cargo test -p gradiance-scene`,
  fmt, clippy.
- **For the next session:** the answers to the six questions change PR #239
  (plan § 13, brief U-1) and PR #240 (`rust-toolchain.toml`) only where the
  owner does not pick the recommended option.
