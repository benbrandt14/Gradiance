# Review mode: open questions as scenes
Phase 4b · owner direction 2026-11-16: "instead of asking me questions here,
create a debug mode where open questions may be presented to me as scenes
with a description, possibly as side-by-side views. This allows me to
interact directly with the running system and make selections. The result
should be saved in some text file I can commit; which is ingested on the
next phase and drawn into the roadmap." · status: ready (runs before 4b-1)

## Goal

Product decisions reach the owner **inside the running app**, not in
chat:

1. A **questions file** in the repo (e.g. `docs/review/questions.ron`; the
   name is yours) lists open questions. Each question has:
   - an id;
   - a title;
   - a description (why it matters, what each choice implies);
   - 2–4 **options**, each with a label, a caption, and a **scene**: a
     `.scm` startup script and/or a scene `.ron`, plus optional settings
     overrides (e.g. a lighting preset).
2. `gradiance --review [file]` opens **review mode**:
   - a panel lists the questions, with answered ones marked;
   - choosing a question shows its description and loads its options;
   - the owner can flip between options (A/B), and **side by side** where
     the options are visual (see Facts);
   - the scenes are live: the owner can play, pause, drag bodies and change
     settings to judge each option by interacting.
3. The owner **selects** an option, or "none of these" plus free text, and
   may add a note. The app writes the answer to an **answers file**
   (e.g. `docs/review/answers.ron` or `.md`): human-readable, diff-friendly,
   one entry per question with the question id, the choice, the note, a
   timestamp and the app's git revision if available. The owner commits it.
4. **Ingestion:** `docs/orchestration.md` gains a short process section.
   - At each phase start, the orchestrator reads the answers file:
     - each answer becomes a roadmap/brief change;
     - its question moves to an "answered" archive, kept for provenance.
   - Orchestrators and workers **add questions to the file instead of
     asking in chat**, each question with its scenes.

## Read first

- `src/main.rs`: CLI parsing (`--script`, `--resume`, `--remote`).
- `crates/gradiance-persist/src/`: scene load requests → `LoadSceneIntent`.
- `crates/gradiance-script/src/bridge.rs`: `StartupScripts`, hot reload, the
  pre-run-snapshot caveat (a second script can't index the first's spawns).
- `crates/gradiance-ui/src/menu.rs` (`Panels::named`), `panels.rs`
  (`PanelToggle`), `docs/ui-design.md`.
- `docs/orchestration.md` (where the ingestion process goes) and
  `docs/state.md` (the retained list: some items there are owner decisions
  and become seed questions).

## Facts (orchestrator, 2026-11-16, main `2edcc08`)

- **Format and UI may change freely** (owner, 2026-11-16).
- Side-by-side: one avian world. Two ways to show options at once:
  - **(a)** spawn each option's scene offset in x within one world, with
    collision layers or distance keeping them apart, and frame the camera
    over both;
  - **(b)** split the viewport into two cameras over the offset regions.
  The 4-4 Outcome warns that viewports interact badly with SSAO; reuse its
  `full_window_sub_view` approach or keep SSAO off in split view.
  Option-level *settings* (lighting/render presets) are global resources, so
  options that differ in settings can only be A/B-toggled, not shown side by
  side. Pick what works and say what each question type supports.
- Loading an option must not pollute the owner's real scene or undo
  history. Review mode is its own session: load replaces the scene, and
  undo history from review is discarded. Don't autosave over
  `.gradiance-session.ron` in review mode.
- The answers file is written by the app, so a write is a file write in the
  UI or persist layer, not a scene command. Write it atomically (temp +
  rename), and never lose earlier answers.

## Approach

1. Data types and the RON schema for questions and answers. Put them in a
   crate both the app and tests can use; `gradiance-scene` or `persist` are
   candidates. Parse with good errors.
2. `--review` mode: panel, option loading, A/B, side by side, select + note
   → answers file.
3. **Seed questions** from the current open product decisions in
   `docs/state.md`'s retained list, each with real scenes:
   - "start-up scene: empty, or ground + a few bodies?";
   - "joint inspector: floating window vs docked Properties tab";
   - "tangent glyph orientation";
   - "how faint the alignment guide is".
   Pick 3–4 that are genuinely the owner's call and visual.
4. The ingestion process in `docs/orchestration.md`; a pointer in
   `CLAUDE.md`'s session section (one line).
5. Screenshots via `tools/screenshot/`: the review panel, an A/B pair, a
   side-by-side view, and the answers file after a selection.

## Out of scope

Answering the questions (that's the owner), remote/MCP review, multi-user
review, `docs/state.md`.

## Done when

- Tests:
  - questions/answers round-trip and parse errors are clear;
  - selecting an option writes exactly one answer entry and preserves
    earlier ones;
  - review mode doesn't touch the autosave or leave undo history;
  - every seed question's scenes load without script errors (a guard like
    3-6's).
- `docs/review/` (or chosen path) with seed questions; ingestion process
  documented.
- Screenshots described in the PR (not in git). Gates green. Outcome filled.

## Outcome

*(worker fills in)*
