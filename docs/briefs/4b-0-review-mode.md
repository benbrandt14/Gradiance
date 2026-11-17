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

Status: done (worker, 2026-11-16). PR #238.

**Run it:** `cargo run -- --review` (or `gradiance --review path/to/questions.ron`).
The Review window opens at the right edge on the first unanswered question,
loaded on option A, with the sim paused. Answers go to `answers.ron` beside the
questions file. A questions file that does not parse stops the app with
`--review: <file>: <line>:<col>: ...` and exit code 2.

**Files and schema.**
- `docs/review/questions.ron`: `QuestionSet { version: 1, questions }`.
  A `Question` has `id` (lower-case, digits, `-`), `title`, `description`,
  `source`, `side_by_side` and 2–4 `options`. A `ReviewOption` has `label`,
  `caption`, `scene` (.ron), `scripts` (.scm, run in order) and `settings`
  (`SettingsOverride { sim, render, lighting, scenery }`). Each override
  replaces its resource, and fields it omits take their defaults.
  Parsing uses RON `implicit_some`, so a partial override reads
  `settings: (lighting: (ssao: true))`.
- `docs/review/answers.ron`: `AnswerSet { version: 1, answers }`. An
  `Answer` has `question` (the id), `title`, `choice`
  (`Picked(label)` | `NoneOfThese(text)`), `note`, `answered_at` (RFC 3339
  UTC) and `revision` (`git rev-parse --short=12 HEAD` of the checkout
  holding the questions file, if git is available). There is one entry per
  question: answering again replaces that entry in place, and git keeps the
  history.
- `docs/review/archive.ron` (`Archive { version, entries: [ArchiveEntry {
  question, answer, ingested_as }] }`): parsed and tested, written by the
  orchestrator at ingestion. It is not created until the first ingestion.
- Code:
  - `crates/gradiance-scene/src/review.rs`: types, validation, RON.
    Pure, and in `scene` because it already has serde/ron/thiserror and
    the domain settings types, so Cargo.lock does not change.
  - `crates/gradiance-persist/src/review.rs`: `ReviewSession`, the
    messages, the atomic answers write, timestamp, git revision, and the
    autosave skip.
  - `src/review.rs`: the loader, in the app shell, because an option needs
    both `persist` and `script` and no lower crate sees both. This adds
    no dependency edge.
  - `crates/gradiance-ui/src/review.rs`: the window.
  - `ClearHistoryIntent` in `command`.

**Design decisions.**
- **Loading** is a short job, one step per frame. It loads the option's
  `.ron` (or an empty scene that keeps workstation config and defaults
  every scene-content setting) with the overrides applied, via
  `LoadSceneIntent`. Each script then runs one frame apart via
  `ScriptInputs::submit_with_reply`, so script errors land in the
  window's problem list. Running scripts one frame apart works around the
  one-snapshot rule for authors: a second script can `body-index-at` the
  first one's spawns. After 3 settle frames (the settings-edit debounce)
  the job frames the camera and sends `ClearHistoryIntent`, which
  dispatch applies *after* the frame's commands. The scene is
  live afterwards: play, drag, settings, and undo of the owner's own
  edits within an option all work.
- **A/B vs side by side.** Every question supports A/B. Side by side
  (option (a) from the Facts) needs `side_by_side: true`, and validation
  rejects it when any option carries settings overrides, since settings
  are global. Each option is built in turn and captured as a
  `SceneRecord`. `lay_out_side_by_side` then places the options left to
  right with a 2 m gap. The first option stays put; later ones get fresh
  `StableId`s (joints and nodes re-pointed), group numbers shifted past the
  earlier options' groups, and world-pin anchors moved. The merged scene
  loads as one. Distance, not collision layers, keeps the options apart.
  Half-plane grounds span every region, so the layout warns about them
  (side-by-side scenes use static box floors). The camera fits the union of
  the regions into the part of the view the window leaves uncovered. The
  UI publishes the window's width as `ReviewSession::view_inset_right`.
  Floating tags name each region. Split viewports (option (b)) were not
  needed, which avoids the 4-4 SSAO caveat entirely. Flipping A/B within a
  question keeps the camera. Changing question resets it to the start-up
  zoom, centred in the uncovered part.
- **Isolation:**
  - `autosave_on_exit` returns early while a `ReviewSession` exists.
  - Loads go through `LoadSceneIntent` directly, not `LoadSceneRequest`,
    so `LastScenePath` never points at a review scene and Ctrl+S cannot
    overwrite one.
  - History is cleared after every load.
  - The answers write re-reads the file, upserts, and writes a temp file
    then renames it in the same directory. A file that does not parse is
    left untouched and the error is shown.
- **Settings structs** (`SimSettings`, `RenderSettings`,
  `LightingSettings`, `ScenerySettings`) gained container
  `#[serde(default)]`, so a partial override parses. Saved bytes are
  unchanged and the format version did not need a bump.
- **UI:** a floating window, not a dock pane (a session you work through
  and close). It is `"review"` in `Panels::named`, so the View menu and
  `(panel-show "review")` both reach it. Outside review mode its empty
  state says how to start review mode. The note field is free text;
  keyboard input does not reach the app under the screenshot harness, so
  notes were tested headless only.

**Seed questions** (`docs/review/questions.ron`, scenes under
`docs/review/scenes/<id>/`, all `.scm`):
1. `startup-scene` (state.md retained, 3-6): Empty (today) / Ground only /
   Ground + a few bodies. A/B only, because the ground is a half-plane.
2. `fillet-bulge` (retained, 4-2): Accept the bulge / Fix it (exact
   fillet). Side by side. Option B shows the fix's result: a plain
   merge on the flush pair, with the fillet kept on the T.
3. `shading` (input for 4b-3): Toon (today) / Smooth / Toon + SSAO.
   A/B with settings overrides.

Not seeded: "joint inspector floating vs docked", "tangent glyph
orientation" and "how faint the alignment guide is". Each needs an
alternative that does not exist yet (a docked joint tab, a rotated glyph,
a guide-alpha knob), so no scene can show both sides. The process section
says such a question waits until a scene can show it.

**Verified facts.**
- One avian world is enough for side by side: the guard test loads both
  fillet options merged, with disjoint regions and the expected 6 bodies.
- Settings overrides cannot share a world, as stated in the Facts, so
  validation enforces A/B for them.
- Script runs complete in the frame they are submitted (`run_scripts` runs
  before dispatch), so one frame per script is enough.

**Tests.**
- `scene::review`: round-trip, partial overrides, parse errors carry
  `line:col`, every structural rule names the question, side-by-side
  rejects overrides, duplicate ids and versions are rejected, upsert keeps
  order.
- `persist::review`: an answer keeps earlier ones and leaves no temp file,
  a corrupt file is not overwritten, timestamps (including a leap day),
  errors name the file.
- `src/review.rs`: side-by-side layout (no overlap, fresh ids, groups,
  half-plane warning), `fit_scale`.
- `tests/it/review.rs`:
  - every seed option loads with no problems and the expected body count
    (so a merge that silently no-ops fails the test), with undo depth 0;
  - side by side: disjoint regions and the summed body count;
  - overrides apply and do not leak into the next option;
  - an undo after a load changes nothing;
  - no autosave in review mode, with a non-review control;
  - an answer writes exactly one entry, keeps an earlier entry
    byte-for-byte, re-answering replaces that entry, and an unknown label
    is refused.

**Screenshots** (scratchpad, not in git): the review panel, A/B
(startup B/C), fillet single and side by side, shading A/B/C, the answer
form, the answered mark and the saved status. The answers file written by
the screenshot run picked `fillet-bulge` = "Fix it (exact fillet)". The
repo's file was restored afterwards, so the committed `answers.ron` is
empty.

**Follow-ups (not done).**
- Script scenes get random body colours on every load, which weakens a
  pure-look A/B like `shading`. A colour/appearance verb (or deterministic
  spawn colours) would fix it. The question's description warns about it.
- An option override for `CameraRig` (e.g. a start orbit) would let a
  shading question open tilted, where the options actually differ.
- The three unseeded questions above, once a variant exists.
- Review mode keeps `register-action` entries and `label` tags from
  earlier options; harmless now, worth a reset if seed scripts start
  registering things.
